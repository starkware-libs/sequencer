#[cfg(feature = "stwo_proving")]
use cairo_vm::types::builtin_name::BuiltinName;
use serde_json::json;
use starknet_api::transaction::TransactionHash;

use super::*;
use crate::errors::VirtualBlockExecutorError;
use crate::server::metrics::outcomes;

const UPSTREAM_MESSAGE: &str = "Invalid transaction nonce";

#[cfg(feature = "stwo_proving")]
const UNSUPPORTED_BUILTINS_MESSAGE: &str = "Transaction uses builtins the prover does not \
                                            support: add_mod (2 instances), mul_mod (3 instances)";

fn reverted_transaction_error(revert_reason: &str) -> RunnerError {
    RunnerError::VirtualBlockExecutor(VirtualBlockExecutorError::TransactionReverted(
        TransactionHash::default(),
        revert_reason.to_string(),
    ))
}

#[cfg(feature = "stwo_proving")]
fn unsupported_builtins_error() -> ProvingError {
    ProvingError::UnsupportedBuiltins {
        unsupported_builtins: vec![(BuiltinName::add_mod, 2), (BuiltinName::mul_mod, 3)],
    }
}

#[cfg(feature = "stwo_proving")]
#[test]
fn test_unsupported_builtins_display() {
    assert_eq!(unsupported_builtins_error().to_string(), UNSUPPORTED_BUILTINS_MESSAGE);
}

#[cfg(feature = "stwo_proving")]
#[test]
fn test_unsupported_builtins_maps_to_unsupported_builtin() {
    let error_object: ErrorObjectOwned =
        VirtualSnosProverError::ProvingError(unsupported_builtins_error()).into();

    assert_eq!(error_object.code(), 1002);
    assert_eq!(error_object.message(), "Unsupported builtin");
    let data: String =
        serde_json::from_str(error_object.data().expect("Missing error data").get()).unwrap();
    assert_eq!(data, UNSUPPORTED_BUILTINS_MESSAGE);
}

#[cfg(feature = "stwo_proving")]
#[test]
fn test_prover_execution_maps_to_internal_error() {
    let error_object: ErrorObjectOwned = VirtualSnosProverError::ProvingError(
        ProvingError::ProverExecution("prover failure".to_string()),
    )
    .into();

    assert_eq!(error_object.code(), -32603);
}

#[test]
fn blocking_check_unavailable_reaches_the_client_as_transaction_blocked() {
    assert_eq!(
        ErrorObjectOwned::from(VirtualSnosProverError::BlockingCheckUnavailable),
        ErrorObjectOwned::from(VirtualSnosProverError::TransactionBlocked),
    );
}

/// Pins the full error the client receives for each kind of runner failure, together with its
/// outcome. A failure counts as `failure_runner_client` exactly when the client receives a
/// client-facing JSON-RPC error code instead of -32603.
#[test]
fn runner_failure_client_error_and_outcome_agree() {
    let upstream_data = json!({"expected_nonce": "0x1"});
    let upstream_rpc_error = |code: i64| {
        RunnerError::ProofProvider(ProofProviderError::UpstreamRpcError {
            code,
            message: UPSTREAM_MESSAGE.to_string(),
            data: Some(upstream_data.clone()),
        })
    };
    let forwarded_upstream_error =
        |code: i32| ErrorObjectOwned::owned(code, UPSTREAM_MESSAGE, Some(upstream_data.clone()));
    let out_of_gas_reason = "Out of gas while executing the entry point";
    let cases = [
        (
            reverted_transaction_error(out_of_gas_reason),
            invalid_transaction_input(format!("{out_of_gas_reason}\n\n{OUT_OF_GAS_HINT}")),
            outcomes::FAILURE_RUNNER_CLIENT,
        ),
        (upstream_rpc_error(41), forwarded_upstream_error(41), outcomes::FAILURE_RUNNER_CLIENT),
        (upstream_rpc_error(0), forwarded_upstream_error(0), outcomes::FAILURE_RUNNER_CLIENT),
        (
            upstream_rpc_error(i64::from(i32::MAX)),
            forwarded_upstream_error(i32::MAX),
            outcomes::FAILURE_RUNNER_CLIENT,
        ),
        (
            upstream_rpc_error(-1),
            internal_server_error("Upstream JSON-RPC error (code -1): Invalid transaction nonce"),
            outcomes::FAILURE_RUNNER,
        ),
        (
            upstream_rpc_error(i64::from(i32::MAX) + 1),
            // The text reports a code outside the i32 range as -32603.
            internal_server_error(
                "Upstream JSON-RPC error (code -32603): Invalid transaction nonce",
            ),
            outcomes::FAILURE_RUNNER,
        ),
        (
            reverted_transaction_error("Assertion failed"),
            internal_server_error(reverted_transaction_error("Assertion failed")),
            outcomes::FAILURE_RUNNER,
        ),
        (
            RunnerError::InputGenerationError("mock failure".to_string()),
            internal_server_error("OS Input generation failed: mock failure"),
            outcomes::FAILURE_RUNNER,
        ),
    ];

    for (runner_error, expected_client_error, expected_outcome) in cases {
        let error = VirtualSnosProverError::RunnerError(Box::new(runner_error));
        let error_description = format!("{error:?}");
        assert_eq!(error.metric_outcome(), expected_outcome, "outcome of {error_description}");
        assert_eq!(
            ErrorObjectOwned::from(error),
            expected_client_error,
            "client error of {error_description}"
        );
    }
}
