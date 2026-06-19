# Bug Hunter #3 Findings — apollo_gateway

Crate audited: `apollo_gateway` at `/home/user/sequencer/crates/apollo_gateway/src/`

---

## Bug 1: P2pPropagatorClientError causes false transaction failure response

**File**: `/home/user/sequencer/crates/apollo_gateway/src/errors.rs`, lines 286–292 and 237–240  
**Description**:  
When the mempool successfully accepts a transaction but then fails to propagate it to the P2P layer, it returns `MempoolError::P2pPropagatorClientError`. The two error-conversion functions treat this asymmetrically:

- `mempool_client_result_to_gw_spec_result` (the gateway-spec path): treats `P2pPropagatorClientError` as `Ok(())` — the spec considers P2P propagation best-effort, so failures are non-fatal. The comment says "Not an error from the gateway's perspective."
- `mempool_client_err_to_deprecated_gw_err` (the deprecated gateway path, actually used by `add_tx_inner`): returns `StarknetError::internal_with_signature_logging(...)` — an internal server error.

The `add_tx_inner` function calls `mempool_client_result_to_deprecated_gw_result`, which wraps `mempool_client_err_to_deprecated_gw_err`. So when P2P propagation fails, the gateway returns an internal error to the user *even though the transaction was successfully added to the mempool*. The user retries, causing a duplicate-transaction error on the retry.

**Root Cause**: The deprecated-gateway error conversion function `mempool_client_err_to_deprecated_gw_err` was not updated to match the spec-gateway's treatment of `P2pPropagatorClientError` as a non-fatal event. The same comment ("Not an error from the gateway's perspective") appears in both functions, but only the spec path acts on it correctly.

**Test**:
```rust
// In crates/apollo_gateway/src/errors.rs or gateway_test.rs
// This test demonstrates the behavioral mismatch.
#[cfg(test)]
mod p2p_error_consistency_test {
    use apollo_mempool_types::communication::{MempoolClientError, MempoolClientResult};
    use apollo_mempool_types::errors::MempoolError;
    use starknet_api::transaction::TransactionHash;
    use starknet_api::transaction::fields::TransactionSignature;

    use crate::errors::{
        mempool_client_result_to_deprecated_gw_result,
        mempool_client_result_to_gw_spec_result,
    };

    /// Demonstrates that P2pPropagatorClientError is treated as Ok in the spec path
    /// but as an error in the deprecated gateway path — causing a false failure response
    /// to the caller even though the transaction was already accepted by the mempool.
    #[test]
    fn p2p_propagator_error_treated_inconsistently() {
        let tx_hash = TransactionHash::default();
        let p2p_error: MempoolClientResult<()> =
            Err(MempoolClientError::MempoolError(MempoolError::P2pPropagatorClientError {
                tx_hash,
            }));

        // Spec path: P2P failure is non-fatal — returns Ok
        let spec_result = mempool_client_result_to_gw_spec_result(p2p_error.clone());
        assert!(
            spec_result.is_ok(),
            "Spec path should treat P2pPropagatorClientError as Ok (non-fatal)"
        );

        // Deprecated GW path: P2P failure is treated as an internal error — returns Err
        // This is what add_tx_inner actually uses.
        let sig = TransactionSignature::default();
        let deprecated_result = mempool_client_result_to_deprecated_gw_result(&sig, p2p_error);
        assert!(
            deprecated_result.is_err(),
            "Bug: deprecated path returns Err for P2pPropagatorClientError, \
             causing a false failure response to the user even though the tx was accepted by mempool"
        );
    }
}
```
**How to verify**: `SEED=0 cargo test -p apollo_gateway p2p_propagator_error_treated_inconsistently`

The test demonstrates the asymmetry. The bug manifests in production when the P2P propagator is temporarily unavailable: the transaction lands in the mempool but the user gets an internal error and retries, creating a duplicate-nonce or duplicate-tx error.

---

## Bug 2: Nonce range check silently wraps when account nonce is near the field prime

**File**: `/home/user/sequencer/crates/apollo_gateway/src/stateful_transaction_validator.rs`, line 289  
**Description**:  
In `validate_nonce`, the upper bound of the allowed nonce window is computed as:
```rust
let max_allowed_nonce =
    Nonce(account_nonce.0 + Felt::from(self.config.max_allowed_nonce_gap));
```
`Felt` is a field element over the STARK prime (`p = 2^251 + 17·2^192 + 1`). Addition wraps modulo `p`. If `account_nonce.0` is close to `p`, then `account_nonce.0 + Felt::from(max_allowed_nonce_gap)` wraps to a tiny value. The subsequent check:
```rust
if !(account_nonce <= incoming_tx_nonce && incoming_tx_nonce <= max_allowed_nonce)
```
then compares a very large `account_nonce` against a tiny `max_allowed_nonce`, causing even `incoming_tx_nonce == account_nonce` to fail (since `account_nonce <= max_allowed_nonce` is false after wrap). An account with a nonce at or near the field prime is permanently locked out: every transaction it submits is rejected with `InvalidTransactionNonce` regardless of the nonce value used.

The `Nonce::try_increment` method (in `starknet_api`) already demonstrates the correct pattern — checking for overflow — but `validate_nonce` does not use it.

**Root Cause**: Unchecked Felt arithmetic. The code should use saturating or checked addition, or clamp `max_allowed_nonce` to the field's maximum representable value when overflow would occur.

**Test**:
```rust
// This test can be placed in stateful_transaction_validator_test.rs
#[cfg(test)]
mod nonce_overflow_test {
    use starknet_api::core::Nonce;
    use starknet_api::executable_transaction::AccountTransaction;
    use starknet_api::test_utils::invoke::executable_invoke_tx;
    use starknet_api::transaction::fields::ValidResourceBounds;
    use starknet_api::{invoke_tx_args, nonce};
    use starknet_types_core::felt::Felt;

    use crate::stateful_transaction_validator::StatefulTransactionValidator;
    use crate::gateway_fixed_block_state_reader::MockGatewayFixedBlockStateReader;
    use crate::state_reader_test_utils::TestStateReader;
    use apollo_gateway_config::config::StatefulTransactionValidatorConfig;
    use blockifier::context::ChainInfo;

    /// The STARK prime minus one — the largest valid Felt value.
    /// p = 0x0800000000000011000000000000000000000000000000000000000000000001
    fn felt_prime_minus_one() -> Felt {
        // P - 1 as Felt. Since P is the modulus, P ≡ 0, so P-1 is the max representable value.
        Felt::from_hex_unchecked(
            "0x0800000000000011000000000000000000000000000000000000000000000000",
        )
    }

    /// Demonstrates that an account whose nonce is at the field prime minus one
    /// cannot submit any transaction, because max_allowed_nonce wraps to a tiny value
    /// and the range check always fails.
    #[tokio::test]
    async fn nonce_at_field_max_wraps_and_rejects_valid_nonce() {
        let account_nonce = Nonce(felt_prime_minus_one());
        // The transaction uses exactly the account nonce — this should always be valid.
        let incoming_nonce = account_nonce;

        let executable_tx: AccountTransaction = executable_invoke_tx(invoke_tx_args!(
            nonce: incoming_nonce,
            resource_bounds: ValidResourceBounds::create_for_testing(),
        ));

        let mut mock_gateway_fixed_block = MockGatewayFixedBlockStateReader::new();
        mock_gateway_fixed_block
            .expect_get_nonce()
            .return_once(move |_| Ok(account_nonce));

        let stateful_validator: StatefulTransactionValidator<TestStateReader, _> =
            StatefulTransactionValidator {
                config: StatefulTransactionValidatorConfig {
                    max_allowed_nonce_gap: 200,    // default production value
                    validate_resource_bounds: false,
                    ..Default::default()
                },
                chain_info: ChainInfo::create_for_testing(),
                state_reader_and_contract_manager: None,
                gateway_fixed_block_state_reader: mock_gateway_fixed_block,
            };

        use std::sync::Arc;
        use apollo_mempool_types::communication::MockMempoolClient;
        let mut mempool_client = MockMempoolClient::new();
        mempool_client.expect_validate_tx().returning(|_| Ok(()));

        let result = stateful_validator
            .run_pre_validation_checks(&executable_tx, account_nonce, Arc::new(mempool_client))
            .await;

        // Bug: this fails with InvalidTransactionNonce even though
        // incoming_nonce == account_nonce, which is the canonical "correct" nonce.
        assert!(
            result.is_ok(),
            "Expected Ok for incoming_nonce == account_nonce, but got: {:?}",
            result
        );
    }
}
```
**How to verify**: `SEED=0 cargo test -p apollo_gateway nonce_at_field_max_wraps_and_rejects_valid_nonce`

The test will fail (demonstrating the bug) because when `account_nonce ≈ prime`, `max_allowed_nonce = account_nonce + 200 ≈ 199` after field wrap, and `account_nonce <= max_allowed_nonce` evaluates to `false`.

---

## Bug 3: `max_nonce_for_validation_skip` config field is defined but never read

**File**: `/home/user/sequencer/crates/apollo_gateway/src/stateful_transaction_validator.rs`, lines 429–461  
**File**: `/home/user/sequencer/crates/apollo_gateway_config/src/config.rs`, line 256  
**Description**:  
The `StatefulTransactionValidatorConfig` struct contains a field `max_nonce_for_validation_skip: Nonce` (defaulting to `Nonce(Felt::ONE)`). Its doc comment says "Maximum nonce for which the validation is skipped." This is also serialized to the config schema (`config_schema.json`), so it is an operator-visible config field.

However, `skip_stateful_validations` in `stateful_transaction_validator.rs` hardcodes the skip condition:
```rust
if tx.nonce() == Nonce(Felt::ONE) && account_nonce == Nonce(Felt::ZERO) {
```
The hardcoded `Felt::ONE` ignores `max_nonce_for_validation_skip` entirely. The config field has no effect on runtime behavior. If an operator sets `max_nonce_for_validation_skip` to `2` (hoping to skip validation for deploy_account + up to 2 invoke transactions), nothing changes.

**Root Cause**: The `skip_stateful_validations` function was apparently written to hardcode the policy instead of reading `self.config.max_nonce_for_validation_skip`. The config field appears to be a legacy copy from `native_blockifier`'s `py_validator.rs` (which does use it at line 114: `tx_nonce <= self.max_nonce_for_validation_skip`).

**Written justification** (hard to mechanically test without behavioral change):  
The bug is that a config field exists, is serialized, and is documented, but has no effect. Any deployment that sets `max_nonce_for_validation_skip` to a value other than 1 will silently behave as if it is still 1. This is a silent configuration lie, not a crash, so it requires reading the config path and the code path to confirm the mismatch.

To verify: search for all reads of `max_nonce_for_validation_skip` in the gateway crate — there are none. The only reads are in the config serialization and the `native_blockifier` crate, which is a separate component:
```
grep -rn "max_nonce_for_validation_skip" crates/apollo_gateway/
# Output: only config.rs definition lines, zero runtime reads
```

---

## Bug 4: `mempool_client_result_to_gw_spec_result` is dead code

**File**: `/home/user/sequencer/crates/apollo_gateway/src/errors.rs`, line 212  
**Description**:  
The function `mempool_client_result_to_gw_spec_result` is `pub` and returns a `Result<(), GatewaySpecError>`, intended for the GatewaySpec error path. However, it is never called anywhere in the codebase — confirmed by `grep -rn "mempool_client_result_to_gw_spec_result"` returning only the definition site.

The gateway currently uses only the deprecated-gateway error path (`mempool_client_result_to_deprecated_gw_result`). The spec path was apparently planned but never wired up, leaving this function as dead code.

**Root Cause**: Incomplete migration or feature implementation. The gateway exposes both a deprecated and a spec-compliant API, but the spec-compliant error path was partially implemented (this function) without ever being called.

**Impact**: 
1. Dead code is a maintenance burden and can mask the Bug 1 asymmetry described above.  
2. The correct behavior (treating `P2pPropagatorClientError` as non-fatal) is only in the dead function, not in the code that's actually executed.

**How to verify**: `grep -rn "mempool_client_result_to_gw_spec_result" /home/user/sequencer/crates/`  
Only one result — the definition. No callers.

---

## Bug 5: Calldata size check combines proof_facts length, but error message reports the total as "calldata_length"

**File**: `/home/user/sequencer/crates/apollo_gateway/src/stateless_transaction_validator.rs`, lines 154–177  
**Description**:  
`validate_tx_extended_calldata_size` computes:
```rust
RpcTransaction::Invoke(RpcInvokeTransaction::V3(tx)) => {
    tx.calldata.0.len() + tx.proof_facts.0.len()
}
```
and then errors with:
```rust
Err(StatelessTransactionValidatorError::CalldataTooLong {
    calldata_length: total_length,       // <-- this is calldata + proof_facts combined
    max_calldata_length: self.config.max_calldata_length,
})
```

The `calldata_length` field in the error is reported as `total_length` (calldata + proof_facts), but is labeled as `calldata_length`. A user whose transaction is rejected sees "Calldata length exceeded maximum: length N (allowed length: M)" where N is the sum of their calldata AND their proof_facts — but the error name says "calldata". Since proof_facts can be up to `max_proof_size` (480,000 elements by default), and `max_calldata_length` is 5,000, a valid transaction with 4,900 calldata items and 200 proof_facts elements will be rejected even though both are individually within their respective limits.

**Root Cause**: The function conflates two distinct limits into one check: calldata size and proof_facts size. The proof_facts size is separately limited by `validate_proof_size`, but the combined total is also constrained by `max_calldata_length`. This creates an invisible combined-total constraint that isn't surfaced in the config, the error message, or the documentation in a way users can understand or anticipate.

**Test**:
```rust
#[cfg(test)]
mod calldata_size_ambiguity_test {
    use apollo_gateway_config::config::StatelessTransactionValidatorConfig;
    use starknet_api::transaction::fields::{
        AllResourceBounds, ProofFacts, ResourceBounds,
    };
    use starknet_api::{calldata, felt, proof_facts};
    use starknet_types_core::felt::Felt;

    use crate::errors::StatelessTransactionValidatorError;
    use crate::stateless_transaction_validator::StatelessTransactionValidator;
    use crate::test_utils::{rpc_tx_for_testing, RpcTransactionArgs, TransactionType, NON_EMPTY_RESOURCE_BOUNDS};

    /// A transaction whose calldata is within max_calldata_length and whose proof_facts is within
    /// max_proof_size, but whose COMBINED length exceeds max_calldata_length, gets rejected with a
    /// misleading "calldata too long" error that blames calldata even though calldata alone is fine.
    #[test]
    fn proof_facts_pushes_combined_total_over_calldata_limit() {
        let max_calldata_length = 5;
        let max_proof_size = 10;

        let config = StatelessTransactionValidatorConfig {
            validate_resource_bounds: false,
            max_calldata_length,   // 5 elements
            max_proof_size,        // 10 elements
            allow_client_side_proving: true,
            ..StatelessTransactionValidatorConfig::default()
        };
        let validator = StatelessTransactionValidator { config };

        // 4 calldata elements — well within max_calldata_length (5).
        let calldata = calldata![Felt::ONE, Felt::ONE, Felt::ONE, Felt::ONE];
        // 3 proof_facts elements — well within max_proof_size (10).
        let proof_facts = proof_facts![Felt::ONE, Felt::ONE, Felt::ONE];

        let rpc_tx_args = RpcTransactionArgs {
            calldata,
            proof_facts,
            resource_bounds: AllResourceBounds {
                l2_gas: NON_EMPTY_RESOURCE_BOUNDS,
                ..Default::default()
            },
            ..Default::default()
        };
        let tx = rpc_tx_for_testing(TransactionType::Invoke, rpc_tx_args);

        // Bug: combined calldata+proof_facts (4+3=7) > max_calldata_length (5),
        // so the tx is rejected even though calldata alone (4) is within limits.
        let err = validator.validate(&tx).unwrap_err();
        assert!(
            matches!(
                err,
                StatelessTransactionValidatorError::CalldataTooLong {
                    calldata_length: 7,  // reported as "calldata_length" but is actually calldata+proof_facts
                    max_calldata_length: 5,
                }
            ),
            "Expected CalldataTooLong(7, 5) — got {:?}",
            err
        );
    }
}
```
**How to verify**: `SEED=0 cargo test -p apollo_gateway proof_facts_pushes_combined_total_over_calldata_limit`

The test confirms the behavior: a user with 4 calldata elements and 3 proof_facts elements gets "Calldata too long: 7" even though the calldata itself (4) is within the declared limit (5). The combined-total semantic is not reflected in the config name, the error field name, or the error message.
