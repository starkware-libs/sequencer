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
use crate::server::errors::internal_server_error;

fn error_data(err: VirtualSnosProverError) -> String {
    let rpc_err: jsonrpsee::types::ErrorObjectOwned = err.into();
    let raw_value = rpc_err.data().expect("expected error data to be present");
    serde_json::from_str(raw_value.get()).expect("error data should be valid JSON string")
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
