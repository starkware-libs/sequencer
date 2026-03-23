use starknet_api::felt;
use starknet_api::transaction::TransactionHash;
use starknet_os::io::os_output::OsOutputError;

use crate::errors::{
    ClassesProviderError,
    ProofProviderError,
    RunnerError,
    VirtualBlockExecutorError,
    VirtualSnosProverError,
};
use crate::server::errors::{internal_server_error, service_busy};

fn error_data(err: VirtualSnosProverError) -> String {
    let rpc_err: jsonrpsee::types::ErrorObjectOwned = err.into();
    let raw_value = rpc_err.data().expect("expected error data to be present");
    serde_json::from_str(raw_value.get()).expect("error data should be valid JSON string")
}

fn error_code(err: VirtualSnosProverError) -> i32 {
    let rpc_err: jsonrpsee::types::ErrorObjectOwned = err.into();
    rpc_err.code()
}

#[test]
fn test_transaction_reverted_produces_user_friendly_message() {
    let tx_hash = TransactionHash(felt!("0x123"));
    let inner = VirtualBlockExecutorError::TransactionReverted(tx_hash, "out of gas".to_string());
    let err = VirtualSnosProverError::from(Box::new(RunnerError::from(inner)));

    let data = error_data(err);
    assert!(data.contains("Transaction reverted"), "Expected 'Transaction reverted' in: {data}");
    assert!(
        !data.contains("VirtualBlockExecutorError"),
        "Should not expose internal type name in: {data}"
    );
}

#[test]
fn test_transaction_execution_error_produces_user_friendly_message() {
    let inner =
        VirtualBlockExecutorError::TransactionExecutionError("execution failed".to_string());
    let err = VirtualSnosProverError::from(Box::new(RunnerError::from(inner)));

    let data = error_data(err);
    assert_eq!(data, "Transaction execution failed.", "Unexpected error data: {data}");
}

#[test]
fn test_state_unavailable_produces_user_friendly_message() {
    let inner = VirtualBlockExecutorError::StateUnavailable;
    let err = VirtualSnosProverError::from(Box::new(RunnerError::from(inner)));

    let data = error_data(err);
    assert!(
        data.contains("Failed to read block state"),
        "Expected 'Failed to read block state' in: {data}"
    );
    assert!(
        !data.contains("StateUnavailable"),
        "Should not expose internal variant name in: {data}"
    );
}

#[test]
fn test_classes_provider_error_produces_user_friendly_message() {
    let inner = ClassesProviderError::GetClassesError("network timeout".to_string());
    let err = VirtualSnosProverError::from(Box::new(RunnerError::from(inner)));

    let data = error_data(err);
    assert!(
        data.contains("Failed to fetch contract classes"),
        "Expected 'Failed to fetch contract classes' in: {data}"
    );
}

#[test]
fn test_proof_provider_error_produces_user_friendly_message() {
    let inner = ProofProviderError::InvalidStateDiff("state diff mismatch".to_string());
    let err = VirtualSnosProverError::from(Box::new(RunnerError::from(inner)));

    let data = error_data(err);
    assert!(
        data.contains("Failed to fetch storage proofs"),
        "Expected 'Failed to fetch storage proofs' in: {data}"
    );
}

#[test]
fn test_transaction_hash_error_produces_user_friendly_message() {
    let err = VirtualSnosProverError::from(Box::new(RunnerError::TransactionHashError(
        "unsupported hash version".to_string(),
    )));

    let data = error_data(err);
    assert_eq!(data, "Failed to compute transaction hash.", "Unexpected error data: {data}");
}

#[test]
fn test_input_generation_error_produces_generic_internal_error() {
    let err = VirtualSnosProverError::from(Box::new(RunnerError::InputGenerationError(
        "missing field".to_string(),
    )));

    let data = error_data(err);
    assert!(
        data.contains("Internal proving error"),
        "Expected 'Internal proving error' in: {data}"
    );
}

#[test]
fn test_output_parse_error_produces_generic_internal_error() {
    let inner = OsOutputError::MissingFieldInOutput("some_field".to_string());
    let err = VirtualSnosProverError::from(inner);

    let data = error_data(err);
    assert!(
        data.contains("Internal proving error"),
        "Expected 'Internal proving error' in: {data}"
    );
}

// Sanity check that the internal_server_error helper itself produces a data field.
#[test]
fn test_internal_server_error_has_data_field() {
    let rpc_err = internal_server_error("some detail");
    assert!(rpc_err.data().is_some(), "internal_server_error should always include a data field");
}

#[test]
fn test_invalid_transaction_type_produces_unsupported_tx_version() {
    let err = VirtualSnosProverError::InvalidTransactionType("unsupported version".to_string());
    let rpc_err: jsonrpsee::types::ErrorObjectOwned = err.into();

    assert_eq!(rpc_err.code(), 61);
    assert_eq!(rpc_err.message(), "The transaction version is not supported");
}

#[test]
fn test_invalid_transaction_input_produces_code_1000() {
    let err = VirtualSnosProverError::InvalidTransactionInput("bad input".to_string());
    let rpc_err: jsonrpsee::types::ErrorObjectOwned = err.into();

    assert_eq!(rpc_err.code(), 1000);
    assert_eq!(rpc_err.message(), "Invalid transaction input");
}

#[test]
fn test_pending_block_validation_error_produces_block_not_found() {
    let err = VirtualSnosProverError::ValidationError(
        "Pending blocks are not supported in this context".to_string(),
    );
    let rpc_err: jsonrpsee::types::ErrorObjectOwned = err.into();

    assert_eq!(rpc_err.code(), 24);
    assert_eq!(rpc_err.message(), "Block not found");
}

#[test]
fn test_non_pending_validation_error_produces_validation_failure() {
    let err = VirtualSnosProverError::ValidationError("some other validation error".to_string());
    let rpc_err: jsonrpsee::types::ErrorObjectOwned = err.into();

    assert_eq!(rpc_err.code(), 55);
    assert_eq!(rpc_err.message(), "Account validation failed");
}

#[test]
fn test_bouncer_lock_error_produces_internal_error() {
    let inner = VirtualBlockExecutorError::BouncerLockError("lock poisoned".to_string());
    let err = VirtualSnosProverError::from(Box::new(RunnerError::from(inner)));

    // Internal error code is -32603.
    assert_eq!(error_code(err), -32603);
}

#[test]
fn test_reexecution_error_produces_internal_error() {
    use blockifier_reexecution::errors::ReexecutionError;
    let inner = VirtualBlockExecutorError::ReexecutionError(Box::new(
        ReexecutionError::AmbiguousChainIdFromUrl("http://example.com".to_string()),
    ));
    let err = VirtualSnosProverError::from(Box::new(RunnerError::from(inner)));

    assert_eq!(error_code(err), -32603);
}

#[test]
fn test_service_busy_has_correct_code_and_message() {
    let rpc_err = service_busy(2);

    assert_eq!(rpc_err.code(), -32005);
    assert_eq!(rpc_err.message(), "Service is busy");
    let raw_value = rpc_err.data().expect("service_busy should include a data field");
    let data: String =
        serde_json::from_str(raw_value.get()).expect("data should be a valid JSON string");
    assert!(data.contains("2 concurrent"), "Expected '2 concurrent' in: {data}");
}
