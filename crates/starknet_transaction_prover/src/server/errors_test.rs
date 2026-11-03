use blockifier_reexecution::errors::RPCStateReaderError;
#[cfg(feature = "stwo_proving")]
use cairo_vm::types::builtin_name::BuiltinName;
use jsonrpsee::types::ErrorObjectOwned;
use rstest::rstest;
use serde_json::json;
use starknet_api::core::ClassHash;
use starknet_api::transaction::TransactionHash;
use starknet_rust::providers::jsonrpc::{HttpTransportError, JsonRpcClientError};
use starknet_rust::providers::ProviderError;

use super::OUT_OF_GAS_HINT;
#[cfg(feature = "stwo_proving")]
use crate::errors::ProvingError;
use crate::errors::{
    ClassesProviderError,
    ProofProviderError,
    RunnerError,
    VirtualBlockExecutorError,
    VirtualSnosProverError,
};

const SECRET_PATH_SEGMENT: &str = "/v2/secret-api-key";

/// Returns the error of a request to a closed local port. Its text includes the request URL.
async fn connection_refused_error() -> reqwest::Error {
    // Without a proxy the request reaches the closed port and fails with a connect error.
    reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .post(format!("http://127.0.0.1:1{SECRET_PATH_SEGMENT}"))
        .send()
        .await
        .expect_err("nothing listens on port 1")
}

fn client_error(runner_error: RunnerError) -> ErrorObjectOwned {
    VirtualSnosProverError::from(Box::new(runner_error)).into()
}

fn internal_error_with_data(data: &str) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(-32603, "Internal error", Some(data))
}

#[tokio::test]
async fn test_transport_errors_do_not_leak_the_node_url() {
    let runner_errors = [
        RunnerError::VirtualBlockExecutor(VirtualBlockExecutorError::ReexecutionError(Box::new(
            RPCStateReaderError::ReqwestError(connection_refused_error().await).into(),
        ))),
        RunnerError::ClassesProvider(ClassesProviderError::StateError(
            RPCStateReaderError::ReqwestError(connection_refused_error().await).into(),
        )),
        RunnerError::ProofProvider(ProofProviderError::from(ProviderError::from(
            JsonRpcClientError::TransportError(HttpTransportError::Reqwest(
                connection_refused_error().await,
            )),
        ))),
    ];

    for runner_error in runner_errors {
        // The check below is only meaningful when the source error text holds the URL.
        assert!(runner_error.to_string().contains(SECRET_PATH_SEGMENT));
        let client_error_json = serde_json::to_string(&client_error(runner_error)).unwrap();
        assert!(
            !client_error_json.contains(SECRET_PATH_SEGMENT),
            "URL leaked: {client_error_json}"
        );
    }
}

#[rstest]
#[case::upstream_application_error(
    RunnerError::ProofProvider(ProofProviderError::UpstreamRpcError {
        code: 41,
        message: "Transaction execution error".to_string(),
        data: Some(json!({"execution_error": "Nonce too old"})),
    }),
    ErrorObjectOwned::owned(
        41,
        "Transaction execution error",
        Some(json!({"execution_error": "Nonce too old"})),
    )
)]
#[case::upstream_infrastructure_error(
    RunnerError::ProofProvider(ProofProviderError::UpstreamRpcError {
        code: -32601,
        message: "Method not found".to_string(),
        data: None,
    }),
    internal_error_with_data("Upstream JSON-RPC error (code -32601): Method not found")
)]
#[case::out_of_gas_revert(
    RunnerError::VirtualBlockExecutor(VirtualBlockExecutorError::TransactionReverted(
        TransactionHash::default(),
        "Out of gas".to_string(),
    )),
    ErrorObjectOwned::owned(
        1000,
        "Invalid transaction input",
        Some(format!("Out of gas\n\n{OUT_OF_GAS_HINT}")),
    )
)]
#[case::deprecated_class(
    RunnerError::ClassesProvider(ClassesProviderError::DeprecatedContractError(ClassHash::default())),
    internal_error_with_data(
        &ClassesProviderError::DeprecatedContractError(ClassHash::default()).to_string(),
    )
)]
#[case::revert(
    RunnerError::VirtualBlockExecutor(VirtualBlockExecutorError::TransactionReverted(
        TransactionHash::default(),
        "Entry point not found".to_string(),
    )),
    internal_error_with_data(&format!(
        "Reverted transactions are not supported; hash: {:?}, revert reason: Entry point not found",
        TransactionHash::default()
    ))
)]
#[case::execution_error(
    RunnerError::VirtualBlockExecutor(VirtualBlockExecutorError::TransactionExecutionError(
        "Entry point not found".to_string(),
    )),
    internal_error_with_data("Transaction execution failed: Entry point not found")
)]
#[case::invalid_state_diff(
    RunnerError::ProofProvider(ProofProviderError::InvalidStateDiff(
        "Storage deletion not allowed".to_string(),
    )),
    internal_error_with_data("Invalid state diff: Storage deletion not allowed")
)]
#[case::block_not_found(
    RunnerError::VirtualBlockExecutor(VirtualBlockExecutorError::ReexecutionError(Box::new(
        RPCStateReaderError::BlockNotFound(json!({"params": {"block_id": {"block_number": 1}}}))
            .into(),
    ))),
    internal_error_with_data(
        r#"Block not found for request {"params":{"block_id":{"block_number":1}}}"#,
    )
)]
fn test_runner_error_maps_to_client_error(
    #[case] runner_error: RunnerError,
    #[case] expected_error: ErrorObjectOwned,
) {
    assert_eq!(client_error(runner_error), expected_error);
}

#[cfg(feature = "stwo_proving")]
const UNSUPPORTED_BUILTINS_MESSAGE: &str = "Transaction uses builtins the prover does not \
                                            support: add_mod (2 instances), mul_mod (3 instances)";

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
