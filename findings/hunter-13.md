# Bug Hunter 13 Findings

## Files Examined

- `crates/starknet_api/src/transaction_hash.rs` — hash computation for all transaction types (Invoke/Declare/Deploy/DeployAccount/L1Handler, V0–V3)
- `crates/starknet_api/src/hash.rs` — `starknet_keccak_hash`, L1→L2 message hash, `StateRoots::global_root`
- `crates/starknet_api/src/transaction/fields.rs` — `Fee`, `Tip`, `ResourceBounds`, `ValidResourceBounds`, `GasAmount`
- `crates/starknet_api/src/core.rs` — `calculate_contract_address`, `ChainId`, `PatriciaKey`, `Nonce`
- `crates/starknet_api/src/block_hash/block_hash_calculator.rs` — `calculate_block_hash`, `concat_counts`, `gas_prices_to_hash`
- `crates/starknet_api/src/block_hash/receipt_commitment.rs` — `calculate_receipt_hash`, `chain_gas_consumed`, `calculate_messages_sent_hash`
- `crates/starknet_api/src/block_hash/transaction_commitment.rs` — `calculate_transaction_leaf`
- `crates/starknet_api/src/block_hash/event_commitment.rs` — `calculate_event_hash`
- `crates/starknet_api/src/block_hash/state_diff_hash.rs` — `calculate_state_diff_hash` and helpers
- `crates/starknet_api/src/crypto/patricia_hash.rs` — Patricia tree root computation
- `crates/starknet_api/src/crypto/utils.rs` — `HashChain`, `verify_message_hash_signature`
- `crates/starknet_api/src/abi/abi_utils.rs` — `starknet_keccak`, `get_storage_var_address`
- `crates/starknet_api/src/serde_utils.rs` — hex serialization/deserialization
- `crates/starknet_api/src/execution_resources.rs` — `GasVector`, `GasAmount`, `GasPrice`
- `crates/starknet_api/src/block.rs` — `GasPrice`, `NonzeroGasPrice`, `concat_counts`
- `crates/starknet_api/src/transaction_hash_test.rs` — existing regression tests

---

## Bug 1

**File**: `crates/starknet_api/src/transaction/fields.rs`
**Location**: `Fee::checked_div_ceil`, line ~51–63
**Description**: `checked_div_ceil` panics in debug builds (or silently returns `GasAmount(0)` in release builds, wrapped from `u64::MAX + 1`) when the floor-division quotient equals `u64::MAX` and there is a non-zero remainder. The code computes `(value.0 + 1).into()` without checking for overflow, which is unsound.

**Root Cause**: `checked_div` returns `Some(GasAmount(u64::MAX))` when `floor(fee / price) == u64::MAX` (it fits in `u64`). If the division has a remainder, the ceiling would be `u64::MAX + 1`, which does not fit in `u64`. The code naively does `(value.0 + 1).into()` — but `u64::MAX + 1` wraps to 0 under release-mode arithmetic (or panics in debug), instead of returning `None` to signal that the ceiling is out of range.

**Example values that trigger the bug**:
- `fee = 2 * u64::MAX + 1` as a `u128` (fits in u128: = 2^65 − 1)
- `price = NonzeroGasPrice(2)`
- `floor(fee / price) = u64::MAX` (remainder = 1, so ceiling = `u64::MAX + 1`)
- `checked_div` returns `Some(GasAmount(u64::MAX))`
- `value * price = u64::MAX * 2 = 2^65 − 2 < 2^65 − 1 = fee` → remainder condition is true
- `(u64::MAX + 1_u64)` **overflows** → panic in debug, wraps to 0 in release

**Failing Test**:
```rust
// Place inside `crates/starknet_api/src/transaction_test.rs` (in the existing test module)
// or in a `#[cfg(test)]` block in `crates/starknet_api/src/transaction/fields.rs`.
#[test]
fn test_fee_div_ceil_overflow_at_u64_max_quotient_with_remainder() {
    use crate::block::NonzeroGasPrice;
    use crate::execution_resources::GasAmount;
    use crate::transaction::fields::Fee;

    // floor(fee / price) == u64::MAX  with remainder 1
    // => ceil(fee / price) == u64::MAX + 1, which does NOT fit in u64.
    // Expected: checked_div_ceil returns None (overflow).
    let fee = Fee((u64::MAX as u128) * 2 + 1); // = 2^65 - 1, fits in u128
    let price = NonzeroGasPrice::try_from(2_u8).unwrap();

    // In debug builds this panics at `(value.0 + 1)` overflow.
    // In release builds this returns Some(GasAmount(0)) (wraps to 0), which is wrong.
    // The correct result is None.
    let result = fee.checked_div_ceil(price);
    assert_eq!(result, None, "ceiling of (2^65-1)/2 does not fit in u64; should return None");
}
```

**How to Verify**: `SEED=0 cargo test -p starknet_api test_fee_div_ceil_overflow_at_u64_max_quotient_with_remainder`

In a debug build this test will panic before reaching the assertion (due to `u64` overflow). In a release build the assertion will fail because the function returns `Some(GasAmount(0))` instead of `None`. Either outcome demonstrates the bug.

**Correct Fix**: Check for `value.0 == u64::MAX` before adding 1 and return `None` in that case, since the ceiling overflows `u64`:

```rust
pub fn checked_div_ceil(self, rhs: NonzeroGasPrice) -> Option<GasAmount> {
    self.checked_div(rhs).and_then(|value| {
        if value
            .checked_mul(rhs.into())
            .expect("Multiplying by denominator of floor division cannot overflow.")
            < self
        {
            // Ceiling = floor + 1; if floor == u64::MAX the ceiling doesn't fit.
            value.0.checked_add(1).map(|n| n.into())
        } else {
            Some(value)
        }
    })
}
```

---

## What Was Checked But Found Correct

1. **Transaction hash field orderings** (all versions): Verified against known mainnet regression vectors and SNIP-8 spec. `InvokeV3`, `DeclareV3`, and `DeployAccountV3` use the correct orderings (the difference in nonce/DA-mode ordering between deploy-account and invoke/declare is intentional per the spec).

2. **`get_concat_resource` byte packing**: `[0 (1B) | resource_name (7B) | max_amount/u64 (8B) | max_price/u128 (16B)]` = 32 bytes, correctly forming a field element.

3. **`concat_data_availability_mode`**: Correctly encodes `[0…0 (192b) | nonce_mode (32b) | fee_mode (32b)]` via `fee_mode + (nonce_mode << 32)`.

4. **`starknet_keccak_hash` / `starknet_keccak`**: Both correctly discard 6 MSBs to yield a 250-bit value. The duplicate implementations are identical.

5. **`l1_handler_message_hash`**: Field ordering and byte encoding match the Starknet spec (from_address, to_address, nonce, selector, payload_length, payload).

6. **`calculate_messages_sent_hash`**: Correctly chains `num_messages, from_L2, to_L1, payload_length, ...payload` per the receipt commitment spec.

7. **`chain_gas_consumed`** hardcodes L2 gas as `Felt::ZERO` — this is intentional and clearly commented ("In the current RPC: always 0"). There is a TODO to add L2 gas consumption in 0.14.0+.

8. **`concat_counts` / `extract_event_count_from_concatenated_counts`**: Verified packing and extraction of `tx_count | event_count | state_diff_len | l1_da_bit` against the regression test.

9. **Patricia tree `calculate_root`**: Correctly handles leaves, edge nodes (common zero prefix), and binary splits for sequential indices.

10. **`calculate_contract_address`**: Correctly computes Pedersen hash of prefix + deployer + salt + class_hash + calldata_hash, then takes modulo by `L2_ADDRESS_UPPER_BOUND`.

11. **`Fee::checked_div`**: Correctly returns `None` when `floor(fee/price)` doesn't fit in `u64`.

12. **State diff hash `chain_deployed_contracts` / `chain_nonces` / `chain_storage_diffs`**: Correct sorting, length-prefixing, and type coercions throughout.
