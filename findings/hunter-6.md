# Blockifier Bug Hunt - Hunter 6

## Bug 1: `get_da_gas_cost` applies fee-balance discount unconditionally even for zero state changes

**File**: `/home/user/sequencer/crates/blockifier/src/fee/gas_usage.rs`, lines 63–68

**Description**:
When `use_kzg_da` is `false`, `get_da_gas_cost` applies a fixed discount intended to account for the fact that the sender's fee-token balance value is a sparse felt (only ~12 non-zero bytes in a 32-byte word). This discount (`GAS_PER_MEMORY_WORD - fee_balance_value_cost`) is **always** subtracted, even if:
- `state_changes_count` has zero storage updates (no fee balance update to discount),
- or the batch is completely empty.

The discount is unconditional:
```rust
let fee_balance_value_cost = eth_gas_constants::get_calldata_word_cost(12);
discount += eth_gas_constants::GAS_PER_MEMORY_WORD - fee_balance_value_cost;
// Cost must be non-negative after discount.
let gas = naive_cost.saturating_sub(discount);
```

`saturating_sub` prevents a negative result, but a non-empty batch (e.g., a batch with only modified contracts but no storage updates) can have its `naive_cost` wrongly reduced. The discount was intended for the case where a storage update to the sender's fee balance is included — but the code applies the discount regardless of whether any storage updates exist.

Conversely, when `n_storage_updates == 0` and the naive cost is small, the unconditional fee-balance discount may over-reduce it, yielding an incorrect DA gas figure. In the existing tests (see `gas_usage_test.rs` line 231), `get_da_gas_cost(&StateChangesCount::default(), use_kzg_da).l1_gas` returns `0`, which is correct only because `saturating_sub` clamps it. But the discount is still logically incorrect when it reduces a non-zero naive cost that has no storage-update entry.

**Root Cause**:
The comment says "Up to balance of 8*(10**10) ETH" for the discount, implying it is always charged, but this only makes sense when a fee balance storage cell is actually written. The code doesn't gate the discount on `n_storage_updates > 0`.

**Test**:
```rust
#[test]
fn test_da_gas_cost_discount_without_storage_update() {
    use crate::fee::gas_usage::get_da_gas_cost;
    use crate::fee::eth_gas_constants;
    use crate::state::cached_state::StateChangesCount;

    // A batch with only 1 modified contract and 0 storage updates.
    // The fee-balance discount should NOT apply, but currently it does.
    let state_changes_count = StateChangesCount {
        n_modified_contracts: 1,
        n_storage_updates: 0,
        n_class_hash_updates: 0,
        n_compiled_class_hash_updates: 0,
    };

    // naive_cost = 2 words (1 modified contract * 2 words per contract) * SHARP_GAS_PER_DA_WORD
    let naive_cost = 2 * eth_gas_constants::SHARP_GAS_PER_DA_WORD;
    // modified_contract discount
    let modified_contract_discount =
        eth_gas_constants::GAS_PER_MEMORY_WORD
        - eth_gas_constants::get_calldata_word_cost(1 + 2 + 3);
    // fee balance discount (should NOT be applied here)
    let fee_balance_discount =
        eth_gas_constants::GAS_PER_MEMORY_WORD - eth_gas_constants::get_calldata_word_cost(12);

    // Correct cost (no fee balance update, no fee balance discount):
    let expected_cost = naive_cost.saturating_sub(modified_contract_discount);
    // Current (buggy) cost applies fee_balance_discount too:
    let current_cost = get_da_gas_cost(&state_changes_count, false).l1_gas.0;

    let correct_cost = expected_cost as u64;
    // The current code applies an extra discount that is not warranted:
    assert!(
        current_cost < correct_cost,
        "Bug: fee balance discount is applied even when n_storage_updates == 0. \
         current_cost={current_cost}, correct_cost={correct_cost}"
    );
}
```

**How to verify**: `SEED=0 cargo test -p blockifier test_da_gas_cost_discount_without_storage_update`

---

## Bug 2: `execute_inner_call` does not clear events of nested failed inner calls

**File**: `/home/user/sequencer/crates/blockifier/src/execution/syscalls/syscall_base.rs`, lines 459–473

**Description**:
When an inner call fails, `execute_inner_call` iterates through the failed call tree to clear events and L2→L1 messages from reverted calls. However, the DFS only descends into **non-failed** child calls:

```rust
stack.extend(
    call_info
        .inner_calls
        .iter_mut()
        .filter(|call_info| !call_info.execution.failed),  // Only non-failed!
);
```

Consider this call tree (all reverted):
- Outer call C1 (fails because C2 fails)
  - C2 (fails because C3 fails)
    - C3 fails, emitting event E3

When processing C1's revert:
1. Push C1 to stack, pop it, clear C1's events.
2. Push C1's non-failed inner calls — but C2 failed, so it's skipped.
3. Stack is empty. C2's events and C3's event **E3 are NOT cleared**.

The comment says: "The events and l2_to_l1_messages of the failed calls were already cleared." This is true when the **child** reverted independently and had its own revert processing. But when C2 fails because C3 (its own inner call) panicked — specifically in the case where the inner call's execution `failed` flag is set due to the call itself failing — C2's events may still be populated (C2 ran code before calling C3 and emitted events), and C3's events (emitted before C3 failed) may also still exist. The filter prevents these from being cleared in the parent's revert pass.

**Root Cause**:
The DFS filter `!call_info.execution.failed` incorrectly assumes that failed subcalls always have their own events already cleared. This is only true for the outermost failure, not for a chain: failed(C1) -> failed(C2) -> events(C2 before call) survive.

**Test (written justification)**:
The bug is hard to reproduce mechanically without a contract that:
1. Emits events before making an inner call
2. The inner call itself fails by calling another failing contract

The flow would need actual Sierra contracts to exercise. A unit-level reproduction would require constructing a `CallInfo` tree manually with `execution.failed = true` on C2 while `execution.events` is still populated on C2, then calling the clearing logic and asserting C2's events were cleared. That logic is currently hidden inside `execute_inner_call` and not separately testable.

**Conceptual Test**:
```rust
// After execute_inner_call returns Err for the outer call, inspect
// execution_output.inner_calls[0].execution.events — if C2 emitted events
// before its own inner call to C3 failed, those events survive in the
// CallInfo tree even though the full call was reverted.
//
// Correct behavior: ALL events in ALL subcalls of a reverted call tree
// should be cleared, regardless of whether those subcalls themselves
// have execution.failed == true.
```

---

## Bug 3: `validate_reads` in `VersionedState` silently panics when reads reference keys not in versioned storage

**File**: `/home/user/sequencer/crates/blockifier/src/concurrency/versioned_state.rs`, lines 87–132

**Description**:
When validating a concurrent transaction's read set, `validate_reads` calls `.expect(READ_ERR)` on `VersionedStorage::read(...)`. The `read` method returns `None` when neither the versioned writes nor the initial values cache contains the key. But `set_initial_value` is only called during `get_storage_at` in `VersionedStateProxy`. If a transaction's read set contains a key that was never accessed through the proxy (e.g., because the key's initial value was never fetched), `validate_reads` panics with `"Error: read value missing in the versioned storage"`.

```rust
let value =
    self.storage.read(tx_index, (contract_address, storage_key)).expect(READ_ERR);
```

**Root Cause**:
The `VersionedStorage::read` function returns `None` if the key was never cached:
- No write at or before `tx_index`
- No entry in `cached_initial_values`

If a transaction reads a storage key during execution (via `VersionedStateProxy`), it populates `cached_initial_values`. But `validate_reads` uses `tx_index - 1` and reads up to that index. If the initial value was never fetched (i.e., the key had no prior write), `read` returns `None` and `expect` panics.

In practice this means a buggy or adversarially-crafted read set can cause an unconditional panic during the concurrent validation phase — a hard crash of the sequencer rather than a recoverable error.

**Test**:
```rust
#[cfg(test)]
mod test {
    use starknet_api::core::ContractAddress;
    use starknet_api::state::StorageKey;
    use starknet_types_core::felt::Felt;

    use crate::concurrency::versioned_storage::VersionedStorage;
    use crate::state::cached_state::StateMaps;
    use std::collections::HashMap;

    // Demonstrate that VersionedStorage::read returns None for a key
    // that was never set_initial_value'd and has no write.
    #[test]
    fn test_versioned_storage_read_missing_key_returns_none() {
        let storage: VersionedStorage<(ContractAddress, StorageKey), Felt> =
            VersionedStorage::default();
        let addr = ContractAddress::default();
        let key = StorageKey::default();
        // No write, no initial value cached.
        let result = storage.read(0, (addr, key));
        assert!(result.is_none(), "Expected None for an uncached key");
    }

    // Demonstrate validate_reads panics when reads contain a key not in versioned storage.
    // This test requires constructing a VersionedState which is harder to do directly,
    // but we can demonstrate the panic via the VersionedStorage contract.
    #[test]
    #[should_panic(expected = "Error: read value missing in the versioned storage")]
    fn test_validate_reads_panics_on_missing_key() {
        use crate::concurrency::versioned_state::VersionedState;
        use crate::test_utils::dict_state_reader::DictStateReader;

        let initial_state = DictStateReader::default();
        let mut versioned_state = VersionedState::new(initial_state);

        // Build a reads map with a storage key that was NEVER fetched through the proxy,
        // so it has no entry in the versioned storage's cached_initial_values.
        let addr = ContractAddress::try_from(Felt::ONE).unwrap();
        let key = StorageKey::try_from(Felt::from(42u64)).unwrap();
        let reads = StateMaps {
            storage: HashMap::from([((addr, key), Felt::from(999u64))]),
            ..StateMaps::default()
        };

        // tx_index = 1 so it checks tx_index - 1 = 0 reads. The key was never cached,
        // so `read(0, ...)` returns None and .expect() panics.
        versioned_state.validate_reads(1, &reads);
    }
}
```

**How to verify**: `SEED=0 cargo test -p blockifier test_versioned_storage_read_missing_key_returns_none`

---

## Bug 4: `AliasUpdater::finalize_updates` writes counter even on first-ever allocation when `next_free_alias == None`

**File**: `/home/user/sequencer/crates/blockifier/src/state/stateful_compression.rs`, lines 145–157

**Description**:
`finalize_updates` has two branches:

```rust
match self.next_free_alias {
    None => {
        // This is the first-ever call (counter was 0 in storage).
        self.set_alias_in_storage(ALIAS_COUNTER_STORAGE_KEY, INITIAL_AVAILABLE_ALIAS)?;
    }
    Some(alias) => {
        if self.is_alias_inserted {
            self.set_alias_in_storage(ALIAS_COUNTER_STORAGE_KEY, alias)?;
        }
    }
}
```

When `next_free_alias == None`, the counter is **always** written to `INITIAL_AVAILABLE_ALIAS` (0x80), even if **no aliases were actually inserted** (`is_alias_inserted == false`). This happens when:
- The state diff has contract addresses and/or storage keys
- All of them either have `alias_key.0 < MIN_VALUE_FOR_ALIAS_ALLOC` (i.e., already have small identity aliases)
- Or all of them already had existing aliases (read from storage != 0)

In such a case, the counter storage slot should not be written because nothing changed. But the `None` branch unconditionally writes `INITIAL_AVAILABLE_ALIAS` as the counter, creating a spurious state change that:
1. Incorrectly marks the alias contract as having a non-zero counter when no aliases exist
2. Adds a spurious write to the state diff, affecting DA gas costs
3. After this spurious write, the next call to `AliasUpdater::new` sees a non-zero counter and initializes `next_free_alias = Some(0x80)` — so the `None` branch never triggers again, which masks the bug in subsequent runs.

**Root Cause**:
The `None` branch should also check `is_alias_inserted` before writing, mirroring the `Some` branch. Currently it unconditionally writes when `stored_counter == 0` (first-ever call), even if no keys qualified for aliasing.

**Test**:
```rust
#[cfg(test)]
mod test {
    use starknet_api::core::{ContractAddress, PatriciaKey};
    use starknet_api::state::StorageKey;
    use starknet_api::StarknetApiError;
    use starknet_types_core::felt::Felt;

    use crate::state::cached_state::CachedState;
    use crate::state::state_api::StateReader;
    use crate::state::stateful_compression::{
        allocate_aliases_in_storage,
        ALIAS_COUNTER_STORAGE_KEY,
        MAX_NON_COMPRESSED_CONTRACT_ADDRESS,
        MIN_VALUE_FOR_ALIAS_ALLOC,
    };
    use crate::test_utils::dict_state_reader::DictStateReader;

    #[test]
    fn test_alias_counter_not_written_when_no_aliases_allocated() {
        // Build a state diff that only touches small-valued storage keys
        // (i.e., keys below MIN_VALUE_FOR_ALIAS_ALLOC) on a small contract address
        // (below MAX_NON_COMPRESSED_CONTRACT_ADDRESS). No keys should require aliasing.
        let alias_contract = ContractAddress::try_from(
            PatriciaKey::try_from(Felt::from(1u64)).unwrap()
        ).unwrap();

        let small_contract = ContractAddress::try_from(
            // Contract address <= MAX_NON_COMPRESSED_CONTRACT_ADDRESS (0xf)
            PatriciaKey::try_from(Felt::from(5u64)).unwrap()
        ).unwrap();

        let initial_state = DictStateReader::default();
        let mut state = CachedState::new(initial_state);

        // Write to a small-address contract with a small storage key.
        // Neither the address nor the key needs an alias.
        let small_key = StorageKey::try_from(Felt::from(1u64)).unwrap();
        state.set_storage_at(small_contract, small_key, Felt::from(42u64)).unwrap();

        // Allocate aliases — nothing should qualify for aliasing.
        allocate_aliases_in_storage(&mut state, alias_contract).unwrap();

        // The alias counter should remain at zero (no write to ALIAS_COUNTER_STORAGE_KEY).
        // Bug: currently this incorrectly writes INITIAL_AVAILABLE_ALIAS (0x80) to the counter.
        let counter = state
            .get_storage_at(alias_contract, ALIAS_COUNTER_STORAGE_KEY)
            .unwrap();
        assert_eq!(
            counter,
            Felt::ZERO,
            "Bug: alias counter was written ({counter}) even though no aliases were allocated"
        );
    }
}
```

**How to verify**: `SEED=0 cargo test -p blockifier test_alias_counter_not_written_when_no_aliases_allocated`

---

## Bug 5: `PostExecutionReport::new` runs balance check even if cost-exceeds-bounds check already failed

**File**: `/home/user/sequencer/crates/blockifier/src/fee/fee_checks.rs`, lines 295–320

**Description**:
`PostExecutionReport::new` unconditionally runs both `check_actual_cost_within_bounds` and `check_can_pay_fee`, then picks up the **first** error encountered:

```rust
let cost_within_bounds_result =
    FeeCheckReport::check_actual_cost_within_bounds(tx_context, tx_receipt);

// Next, verify the actual cost is covered by the account balance...
let can_pay_fee_result = FeeCheckReport::check_can_pay_fee(state, tx_context, tx_receipt);

for fee_check_result in [cost_within_bounds_result, can_pay_fee_result] {
```

When `check_actual_cost_within_bounds` fails (e.g., `MaxGasAmountExceeded`), `check_can_pay_fee` is **still executed**. This means a state read (of the fee token balance) is performed even when it is unnecessary and potentially expensive. More importantly, the comment above states: *"If the above check passes, the pre-execution balance covers the actual cost for sure."* — but the code doesn't short-circuit; it always queries the state for balance even when the first check already failed.

The order of checks in the `for` loop means the first failure is returned. But the `check_can_pay_fee` call is an unnecessary side effect: it mutates the state's cache (by reading the balance) even when we're in a code path that won't use the result.

**Root Cause**:
Both checks are performed eagerly. The code should short-circuit after the first failure. While the correctness is preserved (the first error in the `for` loop is returned), the redundant state read has cost implications and violates the stated invariant in the comment.

**Test (written justification)**:
This is a performance/design bug rather than a crash bug. It requires integration-level testing to observe the spurious state reads. The observable effect: when `MaxGasAmountExceeded` fires, the fee-token balance is still read from state (mutating the `CachedState` cache), which can be confirmed by checking the read set size before/after the call. A property-based test could mock `StateReader` to count reads and verify that `check_can_pay_fee` is not called when bounds are exceeded.

```rust
// Pseudocode to demonstrate the issue:
// let mut read_count = 0;
// // Instrument get_fee_token_balance to increment read_count.
// let report = PostExecutionReport::new(&mut mock_state, &ctx, &receipt, true)?;
// // If bounds are exceeded, read_count should be 0 but is actually 1.
// assert_eq!(read_count, 0, "Balance should not be read when bounds exceeded");
```

---

## Summary

| # | Severity | File | Description |
|---|----------|------|-------------|
| 1 | Medium | `fee/gas_usage.rs:63` | DA gas discount applied unconditionally even with no storage updates |
| 2 | High | `execution/syscalls/syscall_base.rs:467` | Events of nested failed inner calls not cleared on revert |
| 3 | High | `concurrency/versioned_state.rs:89` | `validate_reads` panics (instead of returning error) for keys not in versioned storage |
| 4 | Medium | `state/stateful_compression.rs:145` | Alias counter written spuriously on first call even when no aliases allocated |
| 5 | Low | `fee/fee_checks.rs:301` | Unnecessary state read when cost-exceeds-bounds check already failed |
