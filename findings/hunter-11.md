# Bug Hunter 11 Findings

## Files Examined

- `crates/blockifier/src/fee/fee_checks.rs` — FeeCheckReport, gas bounds checking, revert fee logic
- `crates/blockifier/src/fee/fee_utils.rs` — GasVectorToL1GasForFee, balance checks
- `crates/blockifier/src/fee/gas_usage.rs` — DA gas cost computation, onchain data segment length
- `crates/blockifier/src/fee/resources.rs` — ComputationResources, StarknetResources, StateResources
- `crates/blockifier/src/fee/receipt.rs` — TransactionReceipt construction
- `crates/blockifier/src/fee/eth_gas_constants.rs` — Ethereum gas constants used in DA calculations
- `crates/blockifier/src/bouncer.rs` — BouncerWeights, BuiltinInstanceLimits, sierra_gas_to_steps_gas, get_patricia_update_resources, get_tx_weights
- `crates/blockifier/src/concurrency/fee_utils.rs` — fill_sequencer_balance_reads, add_fee_to_sequencer_balance, complete_fee_transfer_flow
- `crates/blockifier/src/transaction/account_transaction.rs` — run_revertible, handle_fee, execute_fee_transfer
- `crates/blockifier/src/blockifier/transaction_executor.rs` — bouncer try_update call
- `crates/blockifier/src/transaction/objects.rs` — summarize, summarize_builtins
- `crates/blockifier/src/execution/call_info.rs` — EventSummary::to_gas_vector, total_charged_computation_units
- `crates/starknet_api/src/execution_resources.rs` — GasAmount, GasVector, to_discounted_l1_gas
- `crates/blockifier/src/state/cached_state.rs` — count_for_fee_charge, AllocatedKeys

---

## Bug 1

**File**: `crates/blockifier/src/fee/gas_usage.rs`

**Location**: `fn get_da_gas_cost`, line 65 (non-KZG branch)

**Description**: The DA gas cost function unconditionally applies a discount for the sequencer's fee token balance storage cell. It subtracts `GAS_PER_MEMORY_WORD - get_calldata_word_cost(12)` (= 240 gas units) from the DA cost of every non-KZG transaction, regardless of whether the transaction has any fee token balance updates at all. For transactions where the sequencer IS the sender (no net fee transfer), or for L1 handler transactions (which have no ERC-20 fee token update in the state changes), this discount is applied to DA cost derived from state changes that do not include a fee balance update word. The result is an artificially reduced DA gas charge.

**Root Cause**: The comment "Up to balance of 8*(10**10) ETH" refers to the observation that the sequencer's fee token balance low word typically has only ~12 significant bytes, allowing a byte-level discount versus a full 32-byte word. However, the discount is added to the global `discount` variable unconditionally, without checking `state_changes_count.n_storage_updates > 0` or whether a fee balance storage entry is actually in the state changes. When `onchain_data_segment_length == 0` (no state changes), `saturating_sub` clamps to zero and hides the issue. But when there ARE state changes unrelated to the fee balance (e.g., the sender is the sequencer and writes to a contract storage slot), the fee-balance discount is incorrectly applied to other data words, undercharging DA gas.

**Failing Test**:

```rust
#[cfg(test)]
mod tests {
    use crate::fee::eth_gas_constants;
    use crate::fee::gas_usage::get_da_gas_cost;
    use crate::state::cached_state::StateChangesCount;

    /// When there are n_storage_updates but no fee-balance update is expected
    /// (e.g. sequencer is the sender), the DA gas should equal the sum of:
    ///   - n_modified_contracts * 2 * SHARP_GAS_PER_DA_WORD (with the per-contract discount)
    ///   - n_storage_updates * 2 * SHARP_GAS_PER_DA_WORD
    ///
    /// The fee-balance discount (240 gas) must NOT be subtracted when there is
    /// no fee-balance storage word in the state changes.
    ///
    /// This test demonstrates that get_da_gas_cost incorrectly applies the fee-
    /// balance discount even when no fee token balance is part of the state diff.
    #[test]
    fn test_da_gas_cost_no_fee_balance_update_should_not_apply_fee_balance_discount() {
        // One contract modified (nonce bump only), one storage update (not a fee balance).
        let state_changes = StateChangesCount {
            n_storage_updates: 1,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 1,
        };

        // With use_kzg_da = false (calldata mode).
        let gas_vector = get_da_gas_cost(&state_changes, false);

        // onchain_data_segment_length = 1*2 (modified contract) + 1*2 (storage) = 4 words.
        // naive_cost = 4 * SHARP_GAS_PER_DA_WORD = 4 * 612 = 2448
        // modified_contract_discount = GAS_PER_MEMORY_WORD - get_calldata_word_cost(6)
        //   = 512 - (6*16 + 26*4) = 512 - (96 + 104) = 512 - 200 = 312
        // discount_from_contracts = 1 * 312 = 312
        // FEE BALANCE DISCOUNT (incorrectly applied) = GAS_PER_MEMORY_WORD - get_calldata_word_cost(12)
        //   = 512 - (12*16 + 20*4) = 512 - (192 + 80) = 512 - 272 = 240
        // total discount (with bug) = 312 + 240 = 552
        // gas (with bug) = saturating_sub(2448, 552) = 1896
        //
        // Correct behavior (no fee-balance discount when no fee balance in state changes):
        // gas (correct) = saturating_sub(2448, 312) = 2136

        let expected_without_fee_balance_discount = {
            let onchain_data_segment_length = 4_usize;
            let naive_cost =
                onchain_data_segment_length * eth_gas_constants::SHARP_GAS_PER_DA_WORD;
            let modified_contract_cost = eth_gas_constants::get_calldata_word_cost(1 + 2 + 3);
            let modified_contract_discount =
                eth_gas_constants::GAS_PER_MEMORY_WORD - modified_contract_cost;
            let discount = 1 * modified_contract_discount;
            // No fee-balance discount: the 1 storage update is NOT a fee balance word.
            naive_cost.saturating_sub(discount) as u64
        };

        // The current implementation (with the bug) will return a lower value because it
        // unconditionally applies the fee-balance discount. This assertion will FAIL because
        // the actual result is 1896, not 2136.
        assert_eq!(
            gas_vector.l1_gas.0,
            expected_without_fee_balance_discount,
            "DA gas cost incorrectly applies fee-balance discount when no fee balance storage \
             word is present. Got {}, expected {}",
            gas_vector.l1_gas.0,
            expected_without_fee_balance_discount
        );
    }
}
```

**How to Verify**: `cargo test -p blockifier test_da_gas_cost_no_fee_balance_update_should_not_apply_fee_balance_discount`

---

## Bug 2

**File**: `crates/blockifier/src/concurrency/fee_utils.rs`

**Location**: `fn fill_sequencer_balance_reads`, line 94 (`assert_eq!(storage_read_values.len(), 4, ...)`) and line 98 (`assert_eq!(storage_read_values[index], Felt::ZERO, ...)`)

**Description**: The concurrent fee transfer fix-up function hard-codes the assumption that the ERC-20 fee token contract reads exactly 4 storage values during a transfer, and that values at indices 2 and 3 (the sequencer's low and high balance) are always `Felt::ZERO` in the concurrent (fake) execution. Both assertions will panic rather than returning an error, crashing the sequencer process for a single malformed or version-upgraded transaction.

The `assert_eq!(storage_read_values[index], Felt::ZERO, ...)` assertion is especially fragile: it asserts the sequencer's concurrent-mode balance is zero. If two transactions from different senders are both processed concurrently and the sequencer's balance has already been partially updated in the concurrent state, the assertion on `Felt::ZERO` will panic, even though this is a legitimate concurrent execution scenario.

**Root Cause**: The concurrent fee transfer protocol assumes a fixed ERC-20 ABI (4 storage reads: sender low, sender high, sequencer low, sequencer high) and relies on the sequencer's balance being initialized to ZERO in the concurrent execution context. These are implicit, undocumented invariants that are asserted at runtime rather than enforced at compile time or handled gracefully. If the fee token contract is ever upgraded to a version with different internal storage access patterns, or if any concurrent path creates a non-zero sequencer balance read, the sequencer will crash with a panic rather than reverting the transaction.

**Failing Test**:

```rust
#[cfg(test)]
mod tests {
    use starknet_types_core::felt::Felt;
    use crate::concurrency::fee_utils::fill_sequencer_balance_reads;
    use crate::execution::call_info::CallInfo;

    /// Demonstrates that fill_sequencer_balance_reads panics when the fee transfer
    /// call info has a non-zero sequencer balance (index 2 or 3) in the concurrent
    /// (fake) execution. This can happen if the concurrent state happens to have
    /// a previously-written sequencer balance that isn't zero.
    #[test]
    #[should_panic(expected = "Sequencer balance should be zero")]
    fn test_fill_sequencer_balance_reads_panics_on_nonzero_concurrent_balance() {
        let mut call_info = CallInfo::default();
        // Simulate 4 storage reads where the sequencer's low balance (index 2)
        // happens to be non-zero in the concurrent execution context.
        call_info.storage_access_tracker.storage_read_values = vec![
            Felt::from(100u64), // sender low balance
            Felt::ZERO,         // sender high balance
            Felt::from(50u64),  // sequencer low balance (non-zero — triggers panic)
            Felt::ZERO,         // sequencer high balance
        ];

        let real_sequencer_balance = (Felt::from(50u64), Felt::ZERO);
        // This panics: "Sequencer balance should be zero"
        fill_sequencer_balance_reads(&mut call_info, real_sequencer_balance);
    }
}
```

**How to Verify**: `cargo test -p blockifier test_fill_sequencer_balance_reads_panics_on_nonzero_concurrent_balance`

---

## Bug 3

**File**: `crates/blockifier/src/fee/resources.rs`

**Location**: `fn total_charged_computation_units`, line 122

**Description**: The `total_charged_computation_units` function for `TrackedResource::SierraGas` computes `self.sierra_gas.0 + self.reverted_sierra_gas.0` as a raw `u64` addition before converting to `usize` via `usize::try_from(...).unwrap()`. If `sierra_gas` and `reverted_sierra_gas` together exceed `u64::MAX`, this addition overflows silently (wraps in debug builds or is undefined behavior in release builds for Rust, but in practice Rust always wraps on overflow in `u64` arithmetic in release mode without the overflow check). The result is a garbage value that would then panic at the `unwrap()` — but with a misleading panic message about usize conversion rather than arithmetic overflow. In debug mode (with overflow checks enabled), this panics with "attempt to add with overflow".

**Root Cause**: The code uses raw `u64` addition instead of `checked_add` or `saturating_add`. The correct implementation should use `self.sierra_gas.checked_add(self.reverted_sierra_gas)` (the `GasAmount::checked_add` method) to properly detect overflow.

Note: This function is `#[cfg(test)]` only, so it cannot cause a production regression. However, it indicates that tests using `total_charged_computation_units` with very large gas values would produce incorrect results silently in release mode or crash confusingly in debug mode.

**Failing Test**:

```rust
#[cfg(test)]
mod tests {
    use starknet_api::execution_resources::GasAmount;
    use crate::execution::contract_class::TrackedResource;
    use crate::fee::resources::ComputationResources;

    /// Demonstrates that total_charged_computation_units silently wraps (in release) or
    /// panics with a confusing message (in debug) when sierra_gas + reverted_sierra_gas
    /// overflows u64.
    #[test]
    fn test_total_charged_computation_units_overflows_on_large_sierra_gas() {
        let resources = ComputationResources {
            sierra_gas: GasAmount(u64::MAX),
            reverted_sierra_gas: GasAmount(1),
            ..Default::default()
        };

        // In debug mode this panics: "attempt to add with overflow"
        // In release mode this silently returns 0 (u64 wraps to 0, usize::try_from(0) = Ok(0))
        // Neither behavior is correct; the expected result is an overflow error or u64::MAX + 1.
        let _result =
            resources.total_charged_computation_units(TrackedResource::SierraGas);
        // If we reach here (release mode), the result would be wrong (0 instead of overflow).
        // The test should never reach here in debug mode.
    }
}
```

**How to Verify**: `RUSTFLAGS="-C overflow-checks=on" cargo test -p blockifier test_total_charged_computation_units_overflows_on_large_sierra_gas`

---

## Near-Bugs Investigated and Ruled Out

### `count_for_fee_charge` counts only 1 storage update for fee balance (not 2 for uint256)
The sender's fee balance is a uint256 split into two storage slots (low + high). The code only pre-counts 1 storage update for the low word. If the fee causes a borrow from the high word, there would be an undercounted storage update. However, this only affects accounts with balance > 2^128 tokens, which is practically impossible, and the comment acknowledges this is an approximation.

### `sierra_gas_to_steps_gas` silent zero clamping
When Cairo primitive gas costs exceed total sierra gas, the steps gas is clamped to zero. This is intentional behavior with debug logging, used to handle rounding/gas estimation inaccuracies.

### `BuiltinInstanceLimits::induced_gas_costs` uses integer division
`cost = floor(proving_gas / limit)` means the per-builtin cost is always rounded down, potentially undercharging by up to `(limit - 1)` gas units per transaction. This is an intentional conservative approximation.

### `comprehensive_state_diff=false` can over-count allocated_keys
In backward-compatibility mode, keys that go 0→nonzero in validation and back to 0 in execution remain in `allocated_keys`. This is intentional backward-compatible behavior.
