use jsonrpsee::types::error::ErrorCode::InternalError;
use jsonrpsee::types::ErrorObjectOwned;
use starknet_api::transaction::TransactionHash;
use starknet_types_core::felt::Felt;

use super::ServiceErrorCode;
use crate::errors::{
    ProofProviderError,
    RunnerError,
    VirtualBlockExecutorError,
    VirtualSnosProverError,
};

fn reverted_transaction_error(reason: &str) -> ErrorObjectOwned {
    let runner_error =
        RunnerError::VirtualBlockExecutor(VirtualBlockExecutorError::TransactionReverted(
            TransactionHash(Felt::ONE),
            reason.to_string(),
        ));
    ErrorObjectOwned::from(VirtualSnosProverError::RunnerError(Box::new(runner_error)))
}

fn upstream_rpc_error(code: i64, data: Option<serde_json::Value>) -> ErrorObjectOwned {
    let runner_error = RunnerError::ProofProvider(ProofProviderError::UpstreamRpcError {
        code,
        message: "upstream says no".to_string(),
        data,
    });
    ErrorObjectOwned::from(VirtualSnosProverError::RunnerError(Box::new(runner_error)))
}

/// Starknet application errors carry non-negative codes and are meaningful to the caller, so they
/// are forwarded verbatim rather than collapsed into -32603.
#[test]
fn upstream_application_error_is_forwarded_verbatim() {
    let data = serde_json::json!({ "expected_nonce": "0x2" });

    let error = upstream_rpc_error(41, Some(data.clone()));

    assert_eq!(error.code(), 41);
    assert_eq!(error.message(), "upstream says no");
    assert_eq!(
        error.data().map(|raw| serde_json::from_str::<serde_json::Value>(raw.get()).unwrap()),
        Some(data)
    );
}

/// Negative codes are JSON-RPC transport/infrastructure failures rather than anything the caller
/// can act on, so the code collapses to -32603. The upstream message is not hidden — it moves
/// into `data`.
#[test]
fn upstream_infrastructure_error_is_hidden_behind_internal_error() {
    let error = upstream_rpc_error(-32000, None);

    assert_eq!(error.code(), InternalError.code());
    assert_ne!(error.message(), "upstream says no");
}

/// A code outside `i32` cannot be a valid JSON-RPC code; it must not wrap around into an
/// unrelated one. `u32::MAX + 42` truncates to a *positive* `i32` (41), so a wrapping cast would
/// forward it as a real application error instead of rejecting it.
#[test]
fn upstream_code_wider_than_i32_falls_back_to_internal_error() {
    let error = upstream_rpc_error(i64::from(u32::MAX) + 42, None);

    assert_eq!(error.code(), InternalError.code());
}

#[test]
fn out_of_gas_revert_keeps_the_upstream_reason_and_appends_the_hint() {
    let reason = "Transaction execution has failed: Out of gas";

    let error = reverted_transaction_error(reason);

    assert_eq!(error.code(), ServiceErrorCode::InvalidTransactionInput.code());
    let data = error.data().expect("out-of-gas errors carry the reason and hint in data");
    let data: String = serde_json::from_str(data.get()).unwrap();
    assert!(data.starts_with(reason), "expected the upstream reason first, got: {data}");
    assert!(data.contains("starknet_estimateFee"), "expected the hint, got: {data}");
}

/// A revert that is not an out-of-gas one has no actionable hint to add, so it stays an internal
/// error rather than being mislabelled as invalid input.
#[test]
fn non_out_of_gas_revert_is_an_internal_error() {
    let error = reverted_transaction_error("Transaction execution has failed: assertion failed");

    assert_eq!(error.code(), InternalError.code());
}

/// A pending block is reported as "block not found" rather than a validation failure, because the
/// prover only proves closed blocks.
#[test]
fn pending_block_validation_error_maps_to_block_not_found() {
    let error = ErrorObjectOwned::from(VirtualSnosProverError::ValidationError(
        "Pending block".to_string(),
    ));

    assert_eq!(error.code(), ServiceErrorCode::BlockNotFound.code());
}

#[test]
fn other_validation_error_maps_to_account_validation_failed() {
    let error = ErrorObjectOwned::from(VirtualSnosProverError::ValidationError(
        "nonce mismatch".to_string(),
    ));

    assert_eq!(error.code(), ServiceErrorCode::AccountValidationFailed.code());
    let data: String = serde_json::from_str(error.data().expect("data carries the reason").get())
        .expect("data is a JSON string");
    assert_eq!(data, "nonce mismatch");
}
