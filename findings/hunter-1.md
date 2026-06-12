# Bug Hunter 1 Findings

## Files Examined

- `crates/apollo_gateway/src/stateless_transaction_validator.rs` — core stateless validation logic
- `crates/apollo_gateway/src/stateful_transaction_validator.rs` — stateful validation, nonce logic
- `crates/apollo_gateway/src/gateway.rs` — main `add_tx` flow
- `crates/apollo_gateway/src/errors.rs` — error types and conversion
- `crates/apollo_gateway/src/sync_state_reader.rs` — state reader
- `crates/apollo_gateway/src/gateway_fixed_block_state_reader.rs` — block state reader
- `crates/apollo_gateway/src/test_utils.rs` — test helpers
- `crates/apollo_gateway/src/stateless_transaction_validator_test.rs` — stateless tests
- `crates/apollo_gateway/src/stateful_transaction_validator_test.rs` — stateful tests
- `crates/apollo_gateway_config/src/config.rs` — configuration defaults

---

## Bug 1

**File**: `crates/apollo_gateway/src/stateless_transaction_validator.rs`  
**Location**: `fn validate_resource_bounds`, lines 71–76  
**Description**: The `min_gas_price` check is applied unconditionally to `l2_gas.max_price_per_unit`, even when `l2_gas.max_amount == 0` (meaning the transaction does not use any L2 gas). This causes the gateway to **incorrectly reject valid transactions** that set only `l1_gas` or only `l1_data_gas` bounds.

**Root Cause**: In Starknet v3 (`AllResources` format), a transaction may legitimately set `l2_gas.max_amount = 0` and `l2_gas.max_price_per_unit = 0` — indicating that the sender intends to pay only in L1 gas. The non-zero fee is already validated one line earlier (the `ZeroResourceBounds` check uses `max_possible_fee`, which correctly sums over all three gas types). The follow-up guard:

```rust
if resource_bounds.l2_gas.max_price_per_unit.0 < self.config.min_gas_price {
    return Err(StatelessTransactionValidatorError::MaxGasPriceTooLow { … });
}
```

does not distinguish between "zero L2 gas requested, so price is irrelevant" and "L2 gas requested at a price that is too low". When `min_gas_price` is greater than zero (the production default is `8_000_000_000`), any transaction with `l2_gas.max_price_per_unit = 0` trips the guard, even though the transaction carries a fully valid L1 gas fee.

**Why the existing tests don't catch it**: Every positive-flow test case for L1-only (`#[case::valid_l1_gas]`, `#[case::valid_l1_data_gas]`) uses `DEFAULT_VALIDATOR_CONFIG_FOR_TESTING`, which sets `min_gas_price: 0`. The guard `0 < 0` is false, so the bug is never triggered in tests. The production config (`StatelessTransactionValidatorConfig::default()`) has `min_gas_price: 8_000_000_000`, so the bug fires in production.

**Failing Test**:

```rust
#[test]
fn test_min_gas_price_incorrectly_rejects_l1_only_transaction() {
    // Production default: validate_resource_bounds = true, min_gas_price = 8_000_000_000.
    let config = StatelessTransactionValidatorConfig::default();
    assert!(config.validate_resource_bounds);
    assert!(
        config.min_gas_price > 0,
        "Test requires a non-zero min_gas_price to expose the bug"
    );

    let tx_validator = StatelessTransactionValidator { config };

    // A V3 transaction that sets only l1_gas bounds; l2_gas is intentionally zero
    // (no L2 gas requested).
    let resource_bounds = AllResourceBounds {
        l1_gas: ResourceBounds {
            max_amount: GasAmount(100),
            max_price_per_unit: GasPrice(1_000_000_000),
        },
        l2_gas: ResourceBounds::default(),      // max_amount=0, max_price_per_unit=0
        l1_data_gas: ResourceBounds::default(), // max_amount=0, max_price_per_unit=0
    };

    let tx = rpc_tx_for_testing(
        TransactionType::Invoke,
        RpcTransactionArgs { resource_bounds, ..Default::default() },
    );

    // The transaction has a non-zero L1 gas fee; it should pass.
    // BUG: returns Err(MaxGasPriceTooLow { gas_price: GasPrice(0), min_gas_price: 8_000_000_000 })
    // because l2_gas.max_price_per_unit (0) < min_gas_price (8_000_000_000).
    assert_matches!(tx_validator.validate(&tx), Ok(()));
}
```

**How to Verify**: Place the test inside `crates/apollo_gateway/src/stateless_transaction_validator_test.rs` (it needs the imports already present in that module) and run:

```
SEED=0 cargo test -p apollo_gateway test_min_gas_price_incorrectly_rejects_l1_only_transaction
```

The test fails with:
```
assertion failed: matches!(tx_validator.validate(&tx), Ok(()))
left: Err(MaxGasPriceTooLow { gas_price: GasPrice(0), min_gas_price: 8000000000 })
```

**Suggested Fix**: Guard the `min_gas_price` check with `l2_gas.max_amount > 0`:

```rust
if resource_bounds.l2_gas.max_amount.0 > 0
    && resource_bounds.l2_gas.max_price_per_unit.0 < self.config.min_gas_price
{
    return Err(StatelessTransactionValidatorError::MaxGasPriceTooLow { … });
}
```

---

## Other Areas Checked (No Bugs Found)

**Nonce arithmetic near Felt field modulus** (`stateful_transaction_validator.rs`, line 289):

```rust
let max_allowed_nonce = Nonce(account_nonce.0 + Felt::from(self.config.max_allowed_nonce_gap));
```

Felt arithmetic wraps modulo the STARK prime (≈ 3.6 × 10^75). If `account_nonce` were within `max_allowed_nonce_gap` of the STARK prime, `max_allowed_nonce` would wrap to a small value and valid transactions would be rejected. This is a theoretical correctness issue but **not a practical bug**: account nonces start at 0 and increment by 1, so reaching values near the STARK prime is physically impossible. The code follows the pattern used elsewhere in the codebase (`try_increment` uses the same unchecked addition).

**`skip_stateful_validations` logic**: Only skips `__validate__` for Invoke transactions with nonce=1 and account nonce=0 (the deploy-account-plus-invoke UX shortcut). Declare transactions correctly fall through to the full nonce validation path. Logic is correct.

**Entry-point sort/uniqueness check**: Uses strict `<` comparison (`pair[0].selector < pair[1].selector`), so duplicate selectors and out-of-order entries are both correctly rejected. Logic is correct.

**Sierra version upper-bound patch override**: `max_sierra_version.0.patch = usize::MAX` before comparison correctly makes any patch version valid within the configured major.minor range. Logic is correct and well-tested.

**Proof consistency check ordering**: `validate_client_side_proving_allowed` is called before `validate_proof_facts_and_proof_consistency`, which is the correct order — proof data is rejected outright when the feature is disabled rather than leaking into the consistency check.

**GCS proof archive idempotency**: The `if_generation_match: Some(0)` flag combined with HTTP 412 treatment as `Ok(())` correctly handles duplicate submissions without treating them as errors.
