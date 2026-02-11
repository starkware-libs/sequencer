use jsonrpsee::types::ErrorObjectOwned;
use rstest::rstest;

use crate::errors::{ClassesProviderError, RunnerError};
use crate::virtual_snos_prover::VirtualSnosProverError;

const BLOCK_NOT_FOUND: i32 = 24;
const VALIDATION_FAILURE: i32 = 55;
const UNSUPPORTED_TX_VERSION: i32 = 61;
const INTERNAL_ERROR: i32 = -32603;

#[rstest]
#[case(
    VirtualSnosProverError::InvalidTransactionType("Declare is unsupported".to_string()),
    UNSUPPORTED_TX_VERSION
)]
#[case(
    VirtualSnosProverError::ValidationError("Pending blocks are not supported".to_string()),
    BLOCK_NOT_FOUND
)]
#[case(
    VirtualSnosProverError::ValidationError("Other validation failure".to_string()),
    VALIDATION_FAILURE
)]
#[case(
    VirtualSnosProverError::RunnerError(Box::new(RunnerError::InputGenerationError(
        "runner failure".to_string()
    ))),
    INTERNAL_ERROR
)]
#[case(
    VirtualSnosProverError::TransactionHashError("hash failure".to_string()),
    INTERNAL_ERROR
)]
fn virtual_snos_errors_map_to_expected_rpc_codes(
    #[case] error: VirtualSnosProverError,
    #[case] expected_code: i32,
) {
    let rpc_error: ErrorObjectOwned = error.into();
    assert_eq!(rpc_error.code(), expected_code);
}

#[test]
fn invalid_transaction_type_preserves_error_context() {
    let message = "DeployAccount is unsupported";
    let rpc_error: ErrorObjectOwned =
        VirtualSnosProverError::InvalidTransactionType(message.to_string()).into();

    let data = rpc_error.data().expect("Error data should be present").get().to_string();
    assert!(data.contains("DeployAccount"));
}

#[test]
fn non_pending_validation_preserves_error_context() {
    let message = "Validation failed";
    let rpc_error: ErrorObjectOwned =
        VirtualSnosProverError::ValidationError(message.to_string()).into();

    let data = rpc_error.data().expect("Error data should be present").get().to_string();
    assert!(data.contains("Validation failed"));
}

#[test]
fn pending_validation_maps_to_block_not_found_without_data() {
    let rpc_error: ErrorObjectOwned =
        VirtualSnosProverError::ValidationError("Pending block".to_string()).into();

    assert_eq!(rpc_error.code(), BLOCK_NOT_FOUND);
    assert!(rpc_error.data().is_none());
}

#[test]
fn runner_class_provider_errors_map_to_internal_error() {
    let rpc_error: ErrorObjectOwned =
        VirtualSnosProverError::RunnerError(Box::new(RunnerError::ClassesProvider(
            ClassesProviderError::GetClassesError("classes failed".to_string()),
        )))
        .into();

    assert_eq!(rpc_error.code(), INTERNAL_ERROR);
}
