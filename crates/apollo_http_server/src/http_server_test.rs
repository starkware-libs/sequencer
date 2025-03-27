use std::net::{IpAddr, Ipv4Addr};
use std::panic::AssertUnwindSafe;

use apollo_gateway_types::communication::{GatewayClientError, MockGatewayClient};
use apollo_gateway_types::errors::{GatewayError, GatewaySpecError};
use apollo_infra_utils::test_utils::{AvailablePorts, TestIdentifier};
use apollo_sequencer_infra::component_client::ClientError;
use axum::body::{Bytes, HttpBody};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use futures::FutureExt;
use jsonrpsee::types::error::ErrorCode;
use jsonrpsee::types::ErrorObjectOwned;
use rstest::rstest;
use serde::Serialize;
use serde_json::Value;
use starknet_api::transaction::TransactionHash;
use starknet_types_core::felt::Felt;
use tracing_test::traced_test;

use crate::config::HttpServerConfig;
use crate::errors::HttpServerError;
use crate::http_server::{add_tx_result_as_json, CLIENT_REGION_HEADER};
use crate::test_utils::{
    deprecated_gateway_tx,
    http_client_server_setup,
    rpc_tx,
    HttpServerEndpoint,
};

#[tokio::test]
async fn test_tx_hash_json_conversion() {
    let tx_hash = TransactionHash::default();
    let response = add_tx_result_as_json(Ok(tx_hash)).into_response();

    let status_code = response.status();
    let response_bytes = &to_bytes(response).await;

    assert_eq!(status_code, StatusCode::OK, "{response_bytes:?}");
    assert_eq!(tx_hash, serde_json::from_slice(response_bytes).unwrap());
}

async fn to_bytes(res: Response) -> Bytes {
    res.into_body().collect().await.unwrap().to_bytes()
}

#[tokio::test]
async fn test_add_tx_result_as_json_negative() {
    let error = HttpServerError::DeserializationError(
        serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err(),
    );
    let response = add_tx_result_as_json(Err(error)).unwrap_err().into_response();

    let status = response.status();
    let body = to_bytes(response).await;
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(status.is_success());
    assert_eq!(json.get("code").unwrap(), ErrorCode::ParseError.code());
    assert_eq!(json.get("message").unwrap().as_str().unwrap(), "Failed to parse the request body.");
}

#[traced_test]
#[rstest]
#[case::add_rest_tx(0, HttpServerEndpoint::AddTransaction, deprecated_gateway_tx())]
#[case::add_rpc_tx(1, HttpServerEndpoint::AddRpcTransaction, rpc_tx())]
#[tokio::test]
/// Test that when an "add_tx" HTTP request is sent to the server, the region of the http request is
/// recorded to the info log.
async fn record_region_test(
    #[case] index: u16,
    #[case] endpoint: HttpServerEndpoint,
    #[case] tx: impl Serialize + Clone,
) {
    let mut mock_gateway_client = MockGatewayClient::new();
    // Set the successful response.
    let tx_hash_1 = TransactionHash(Felt::ONE);
    let tx_hash_2 = TransactionHash(Felt::TWO);
    mock_gateway_client.expect_add_tx().times(1).return_const(Ok(tx_hash_1));
    mock_gateway_client.expect_add_tx().times(1).return_const(Ok(tx_hash_2));

    let ip = IpAddr::from(Ipv4Addr::LOCALHOST);
    let mut available_ports =
        AvailablePorts::new(TestIdentifier::HttpServerUnitTests.into(), 1 + index);
    let http_server_config = HttpServerConfig { ip, port: available_ports.get_next_port() };
    let add_tx_http_client =
        http_client_server_setup(mock_gateway_client, http_server_config).await;

    // Send a transaction to the server, without a region.
    add_tx_http_client.add_tx(tx.clone(), endpoint).await;
    assert!(logs_contain(
        format!("Recorded transaction with hash: {} from region: {}", tx_hash_1, "N/A").as_str()
    ));

    // Send transaction to the server, with a region.
    let region = "test";
    add_tx_http_client.add_tx_with_headers(tx, endpoint, [(CLIENT_REGION_HEADER, region)]).await;
    assert!(logs_contain(
        format!("Recorded transaction with hash: {} from region: {}", tx_hash_2, region).as_str()
    ));
}

#[traced_test]
#[rstest]
#[case::add_rest_tx(0, HttpServerEndpoint::AddTransaction, deprecated_gateway_tx())]
#[case::add_rpc_tx(1, HttpServerEndpoint::AddRpcTransaction, rpc_tx())]
#[tokio::test]
/// Test that when an "add_tx" HTTP request is sent to the server, and it fails in the Gateway, no
/// record of the region is logged.
async fn record_region_gateway_failing_tx(
    #[case] index: u16,
    #[case] endpoint: HttpServerEndpoint,
    #[case] tx: impl Serialize,
) {
    let mut mock_gateway_client = MockGatewayClient::new();
    // Set the failed response.
    mock_gateway_client.expect_add_tx().times(1).return_const(Err(
        GatewayClientError::ClientError(ClientError::UnexpectedResponse(
            "mock response".to_string(),
        )),
    ));

    let ip = IpAddr::from(Ipv4Addr::LOCALHOST);
    let mut available_ports =
        AvailablePorts::new(TestIdentifier::HttpServerUnitTests.into(), 3 + index);
    let http_server_config = HttpServerConfig { ip, port: available_ports.get_next_port() };
    // let http_server_config = HttpServerConfig { ip, port };
    let add_tx_http_client =
        http_client_server_setup(mock_gateway_client, http_server_config).await;

    // Send a transaction to the server.
    add_tx_http_client.add_tx(tx, endpoint).await;
    assert!(!logs_contain("Recorded transaction with hash: "));
}

// TODO(Yael): add rest_api tests for deploy_account and declare
#[rstest]
#[case::add_rest_tx(0, HttpServerEndpoint::AddTransaction, deprecated_gateway_tx())]
#[case::add_rpc_tx(1, HttpServerEndpoint::AddRpcTransaction, rpc_tx())]
#[tokio::test]
async fn test_response(
    #[case] index: u16,
    #[case] endpoint: HttpServerEndpoint,
    #[case] tx: impl Serialize + Clone,
) {
    let mut mock_gateway_client = MockGatewayClient::new();

    // Set the successful response.
    let expected_tx_hash = TransactionHash(Felt::ONE);
    mock_gateway_client.expect_add_tx().times(1).return_const(Ok(expected_tx_hash));

    // Set the failed response.
    let expected_error = GatewaySpecError::ClassAlreadyDeclared;
    let expected_err_str = format!(
        "Gateway responded with: {}",
        serde_json::to_string(&ErrorObjectOwned::from(expected_error.clone().into_rpc())).unwrap()
    );
    mock_gateway_client.expect_add_tx().times(1).return_const(Err(
        GatewayClientError::GatewayError(GatewayError::GatewaySpecError {
            source: expected_error,
            p2p_message_metadata: None,
        }),
    ));

    let ip = IpAddr::from(Ipv4Addr::LOCALHOST);
    let mut available_ports =
        AvailablePorts::new(TestIdentifier::HttpServerUnitTests.into(), 5 + index);
    let http_server_config = HttpServerConfig { ip, port: available_ports.get_next_port() };
    let add_tx_http_client =
        http_client_server_setup(mock_gateway_client, http_server_config).await;

    // Test a successful response.
    let tx_hash = add_tx_http_client.assert_add_tx_success(tx.clone(), endpoint).await;
    assert_eq!(tx_hash, expected_tx_hash);

    // Test a failed response.
    let panicking_task = AssertUnwindSafe(add_tx_http_client.assert_add_tx_success(tx, endpoint));
    let error = panicking_task.catch_unwind().await.unwrap_err().downcast::<String>().unwrap();
    let error_str = format!("{}", error);
    assert_eq!(error_str, expected_err_str);
}
