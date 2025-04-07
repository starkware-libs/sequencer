use apollo_gateway_types::communication::{GatewayClientError, MockGatewayClient};
use apollo_gateway_types::deprecated_gw_error::{
    KnownStarknetErrorCode,
    StarknetError,
    StarknetErrorCode,
};
use apollo_gateway_types::errors::GatewayError;
use apollo_gateway_types::gateway_types::{
    DeclareGatewayOutput,
    DeployAccountGatewayOutput,
    GatewayOutput,
    InvokeGatewayOutput,
};
use apollo_infra::component_client::ClientError;
use axum::body::{Bytes, HttpBody};
use axum::response::{IntoResponse, Response};
use axum::Json;
use hyper::StatusCode;
use rstest::rstest;
use serde_json::Value;
use starknet_api::test_utils::read_json_file;
use starknet_api::transaction::TransactionHash;
use starknet_api::{class_hash, contract_address, tx_hash};
use starknet_types_core::felt::Felt;
use tracing_test::traced_test;

use crate::errors::HttpServerError;
use crate::http_server::CLIENT_REGION_HEADER;
use crate::test_utils::{add_tx_http_client, deprecated_gateway_tx, rpc_tx, GatewayTransaction};

const DEPRECATED_GATEWAY_INVOKE_TX_RESPONSE_JSON_PATH: &str =
    "expected_gateway_response/invoke_gateway_output.json";
const DEPRECATED_GATEWAY_DECLARE_TX_RESPONSE_JSON_PATH: &str =
    "expected_gateway_response/declare_gateway_output.json";
const DEPRECATED_GATEWAY_DEPLOY_ACCOUNT_TX_RESPONSE_JSON_PATH: &str =
    "expected_gateway_response/deploy_account_gateway_output.json";

#[rstest]
#[case::invoke(
    GatewayOutput::Invoke(InvokeGatewayOutput::new(tx_hash!(1_u64))),
    DEPRECATED_GATEWAY_INVOKE_TX_RESPONSE_JSON_PATH,
)]
#[case::declare(
    GatewayOutput::Declare(DeclareGatewayOutput::new(tx_hash!(1_u64), class_hash!(2_u64))),
    DEPRECATED_GATEWAY_DECLARE_TX_RESPONSE_JSON_PATH,

)]
#[case::deploy_account(
    GatewayOutput::DeployAccount(DeployAccountGatewayOutput::new(
        tx_hash!(1_u64),
        contract_address!(3_u64)
    )),
    DEPRECATED_GATEWAY_DEPLOY_ACCOUNT_TX_RESPONSE_JSON_PATH,
)]
#[tokio::test]
async fn gateway_output_json_conversion(
    #[case] gateway_output: GatewayOutput,
    #[case] expected_serialized_response_path: &str,
) {
    let response = Json(gateway_output).into_response();

    let status_code = response.status();
    let response_bytes = &to_bytes(response).await;

    assert_eq!(status_code, StatusCode::OK, "{response_bytes:?}");
    let gateway_response: GatewayOutput = serde_json::from_slice(response_bytes).unwrap();

    let expected_gateway_response =
        serde_json::from_value(read_json_file(expected_serialized_response_path))
            .expect("Failed to deserialize json to GatewayOutput");
    assert_eq!(gateway_response, expected_gateway_response);
}

async fn to_bytes(res: Response) -> Bytes {
    res.into_body().collect().await.unwrap().to_bytes()
}

#[tokio::test]
async fn error_into_response() {
    let error = HttpServerError::DeserializationError(
        serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err(),
    );
    let response = error.into_response();

    let status = response.status();
    let body = to_bytes(response).await;
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(!status.is_success(), "{:?}", status);
    assert_eq!(
        json.get("code").unwrap(),
        &serde_json::to_value(&KnownStarknetErrorCode::MalformedRequest).unwrap()
    );
}

#[traced_test]
#[rstest]
#[case::add_rest_tx(0, deprecated_gateway_tx())]
#[case::add_rpc_tx(1, rpc_tx())]
#[tokio::test]
/// Test that when an add transaction HTTP request is sent to the server, the region of the http
/// request is recorded to the info log.
async fn record_region_test(#[case] index: u16, #[case] tx: impl GatewayTransaction) {
    let mut mock_gateway_client = MockGatewayClient::new();
    // Set the successful response.
    let tx_hash_1 = TransactionHash(Felt::ONE);
    let tx_hash_2 = TransactionHash(Felt::TWO);
    mock_gateway_client
        .expect_add_tx()
        .times(1)
        .return_const(Ok(GatewayOutput::Invoke(InvokeGatewayOutput::new(tx_hash_1))));
    mock_gateway_client
        .expect_add_tx()
        .times(1)
        .return_const(Ok(GatewayOutput::Invoke(InvokeGatewayOutput::new(tx_hash_2))));

    // TODO(Yael): avoid the hardcoded node offset index, consider dynamic allocation.
    let http_client = add_tx_http_client(mock_gateway_client, 1 + index).await;

    // Send a transaction to the server, without a region.
    http_client.add_tx(tx.clone()).await;
    assert!(logs_contain(
        format!("Recorded transaction with hash: {} from region: {}", tx_hash_1, "N/A").as_str()
    ));

    // Send transaction to the server, with a region.
    let region = "test";
    http_client.add_tx_with_headers(tx, [(CLIENT_REGION_HEADER, region)]).await;
    assert!(logs_contain(
        format!("Recorded transaction with hash: {} from region: {}", tx_hash_2, region).as_str()
    ));
}

#[traced_test]
#[rstest]
#[case::add_rest_tx(0, deprecated_gateway_tx())]
#[case::add_rpc_tx(1, rpc_tx())]
#[tokio::test]
/// Test that when an "add_tx" HTTP request is sent to the server, and it fails in the Gateway, no
/// record of the region is logged.
async fn record_region_gateway_failing_tx(#[case] index: u16, #[case] tx: impl GatewayTransaction) {
    let mut mock_gateway_client = MockGatewayClient::new();
    // Set the failed response.
    mock_gateway_client.expect_add_tx().times(1).return_const(Err(
        GatewayClientError::ClientError(ClientError::UnexpectedResponse(
            "mock response".to_string(),
        )),
    ));

    let http_client = add_tx_http_client(mock_gateway_client, 3 + index).await;

    // Send a transaction to the server.
    http_client.add_tx(tx).await;
    assert!(!logs_contain("Recorded transaction with hash: "));
}

// TODO(Yael): add rest_api tests for deploy_account and declare
#[rstest]
#[case::add_rest_tx(0, deprecated_gateway_tx())]
#[case::add_rpc_tx(1, rpc_tx())]
#[tokio::test]
async fn test_response(#[case] index: u16, #[case] tx: impl GatewayTransaction) {
    let mut mock_gateway_client = MockGatewayClient::new();

    // Set the successful response.
    let expected_tx_hash = TransactionHash(Felt::ONE);
    mock_gateway_client
        .expect_add_tx()
        .times(1)
        .return_const(Ok(GatewayOutput::Invoke(InvokeGatewayOutput::new(expected_tx_hash))));

    // Set the failed response.
    let expected_error = StarknetError {
        code: StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::ClassAlreadyDeclared),
        message: "bla".to_string(),
    };
    let expected_err_str = serde_json::to_string(&expected_error).unwrap();
    mock_gateway_client.expect_add_tx().times(1).return_const(Err(
        GatewayClientError::GatewayError(GatewayError::DeprecatedGatewayError {
            source: expected_error,
            p2p_message_metadata: None,
        }),
    ));

    // Set the failed Gateway ClientError response.
    let expected_gateway_client_err_str = serde_json::to_string(&StarknetError {
        code: StarknetErrorCode::UnknownErrorCode(
            "StarknetErrorCode.UNEXPECTED_FAILURE".to_string(),
        ),
        message: "Internal error".to_string(),
    })
    .unwrap();

    mock_gateway_client.expect_add_tx().times(1).return_const(Err(
        GatewayClientError::ClientError(ClientError::UnexpectedResponse(
            "mock response".to_string(),
        )),
    ));

    let http_client = add_tx_http_client(mock_gateway_client, 5 + index).await;

    // Test a successful response.
    let tx_hash = http_client.assert_add_tx_success(tx.clone()).await;
    assert_eq!(tx_hash, expected_tx_hash);

    // Test a failed bad request response.
    let error_str = http_client.assert_add_tx_error(tx.clone(), StatusCode::BAD_REQUEST).await;
    assert_eq!(error_str, expected_err_str);

    // Test a failed internal server error response.
    let error_str = http_client.assert_add_tx_error(tx, StatusCode::INTERNAL_SERVER_ERROR).await;
    assert_eq!(error_str, expected_gateway_client_err_str);
}

#[rstest]
#[case::missing_version(
    0,
    None,
    Err(StarknetError {
        code: StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::MalformedRequest),
        message: "Missing version field".to_string(),
    })
)]
#[case::bad_version(
    1,
    Some("bad version"),
    Err(StarknetError {
        code: StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::MalformedRequest),
        message: "Version field is not a valid hex string: bad version".to_string(),
    })
)]
#[case::old_version(2, Some("0x1"), Err(StarknetError {
            code: StarknetErrorCode::KnownErrorCode(
                KnownStarknetErrorCode::InvalidTransactionVersion,
            ),
            message: "Transaction version 1 is not supported. Supported versions: [3]."
                .to_string(),
        }),
)]
#[case::current_version(3, Some("0x3"), Ok(TransactionHash(Felt::ONE)))]
#[case::newer_version(4, Some("0x4"), Err(StarknetError {
                code: StarknetErrorCode::KnownErrorCode(
                    KnownStarknetErrorCode::InvalidTransactionVersion,
                ),
                message: "Transaction version 4 is not supported. Supported versions: [3]."
                    .to_string(),
            }
))]
#[tokio::test]
async fn test_tx_version(
    #[case] index: u16,
    #[case] version: Option<&str>,
    #[case] expected: Result<TransactionHash, StarknetError>,
) {
    // Set the tx version to the given version.
    let mut tx_json = serde_json::to_value(deprecated_gateway_tx()).unwrap();
    let as_object = tx_json.as_object_mut().unwrap();
    if let Some(version) = version {
        as_object.insert("version".to_string(), Value::String(version.to_string())).unwrap();
    } else {
        as_object.remove("version").unwrap();
    }

    let mut mock_gateway_client = MockGatewayClient::new();
    if let Ok(expected_tx_hash) = expected {
        mock_gateway_client
            .expect_add_tx()
            .times(1)
            .return_const(Ok(GatewayOutput::Invoke(InvokeGatewayOutput::new(expected_tx_hash))));
    }

    let http_client = add_tx_http_client(mock_gateway_client, 6 + index).await;
    match expected {
        Ok(expected_tx_hash) => {
            let tx_hash = http_client.assert_add_tx_success(tx_json.clone()).await;
            assert_eq!(expected_tx_hash, tx_hash);
        }
        Err(expected_err) => {
            let serialized_err =
                http_client.assert_add_tx_error(tx_json.clone(), StatusCode::BAD_REQUEST).await;
            let starknet_error = serde_json::from_str::<StarknetError>(&serialized_err).unwrap();
            assert_eq!(starknet_error, expected_err);
        }
    }
}

// Impl the trait for tx json that doesn't serialize into a valid tx to test the error handling of
// unsupported tx versions.
impl GatewayTransaction for serde_json::Value {
    fn endpoint(&self) -> &str {
        "add_transaction"
    }

    fn content_type(&self) -> &str {
        "application/text"
    }
}
