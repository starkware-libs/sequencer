use std::net::{IpAddr, Ipv4Addr};
use std::panic::AssertUnwindSafe;

use axum::body::{Bytes, HttpBody};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use blockifier_test_utils::cairo_versions::CairoVersion;
use futures::FutureExt;
use jsonrpsee::types::ErrorObjectOwned;
use mempool_test_utils::starknet_api_test_utils::invoke_tx;
use starknet_api::transaction::TransactionHash;
use starknet_gateway_types::communication::{GatewayClientError, MockGatewayClient};
use starknet_gateway_types::errors::{GatewayError, GatewaySpecError};
use starknet_infra_utils::test_utils::{AvailablePorts, TestIdentifier};
use starknet_sequencer_infra::component_client::ClientError;
use starknet_types_core::felt::Felt;
use tracing_test::traced_test;

use crate::config::HttpServerConfig;
use crate::http_server::{add_tx_result_as_json, CLIENT_REGION_HEADER};
use crate::test_utils::http_client_server_setup;

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

#[traced_test]
#[tokio::test]
/// Test that when an "add_tx" HTTP request is sent to the server, the region of the http request is
/// recorded to the info log.
async fn record_region_test() {
    let mut mock_gateway_client = MockGatewayClient::new();
    // Set the successful response.
    let tx_hash_1 = TransactionHash(Felt::ONE);
    let tx_hash_2 = TransactionHash(Felt::TWO);
    mock_gateway_client.expect_add_txs().times(1).return_const(Ok(vec![tx_hash_1]));
    mock_gateway_client.expect_add_txs().times(1).return_const(Ok(vec![tx_hash_2]));

    let ip = IpAddr::from(Ipv4Addr::LOCALHOST);
    let mut available_ports = AvailablePorts::new(TestIdentifier::HttpServerUnitTests.into(), 1);
    let http_server_config = HttpServerConfig { ip, port: available_ports.get_next_port() };
    let add_tx_http_client =
        http_client_server_setup(mock_gateway_client, http_server_config).await;

    // Send a transaction to the server, without a region.
    let rpc_tx = invoke_tx(CairoVersion::default());
    add_tx_http_client.add_tx(rpc_tx).await;
    assert!(logs_contain(
        format!("Recorded transaction with hash: {} from region: {}", tx_hash_1, "N/A").as_str()
    ));

    // Send transaction to the server, with a region.
    let rpc_tx = invoke_tx(CairoVersion::default());
    let region = "test";
    add_tx_http_client.add_tx_with_headers(rpc_tx, [(CLIENT_REGION_HEADER, region)]).await;
    assert!(logs_contain(
        format!("Recorded transaction with hash: {} from region: {}", tx_hash_2, region).as_str()
    ));
}

#[traced_test]
#[tokio::test]
/// Test that when an "add_tx" HTTP request is sent to the server, and it fails in the Gateway, no
/// record of the region is logged.
async fn record_region_gateway_failing_tx() {
    let mut mock_gateway_client = MockGatewayClient::new();
    // Set the failed response.
    mock_gateway_client.expect_add_txs().times(1).return_const(Err(
        GatewayClientError::ClientError(ClientError::UnexpectedResponse(
            "mock response".to_string(),
        )),
    ));

    let ip = IpAddr::from(Ipv4Addr::LOCALHOST);
    let mut available_ports = AvailablePorts::new(TestIdentifier::HttpServerUnitTests.into(), 2);
    let http_server_config = HttpServerConfig { ip, port: available_ports.get_next_port() };
    // let http_server_config = HttpServerConfig { ip, port };
    let add_tx_http_client =
        http_client_server_setup(mock_gateway_client, http_server_config).await;

    // Send a transaction to the server.
    let rpc_tx = invoke_tx(CairoVersion::default());
    add_tx_http_client.add_tx(rpc_tx).await;
    assert!(!logs_contain("Recorded transaction with hash: "));
}

#[tokio::test]
async fn test_response() {
    let mut mock_gateway_client = MockGatewayClient::new();

    // Set the successful response.
    let expected_tx_hash = TransactionHash(Felt::ONE);
    mock_gateway_client.expect_add_txs().times(1).return_const(Ok(vec![expected_tx_hash]));

    // Set the failed response.
    let expected_error = GatewaySpecError::ClassAlreadyDeclared;
    let expected_err_str = format!(
        "Gateway responded with: {}",
        serde_json::to_string(&ErrorObjectOwned::from(expected_error.clone().into_rpc())).unwrap()
    );
    mock_gateway_client.expect_add_txs().times(1).return_const(Err(
        GatewayClientError::GatewayError(GatewayError::GatewaySpecError {
            source: expected_error,
            p2p_message_metadata: None,
        }),
    ));

    let ip = IpAddr::from(Ipv4Addr::LOCALHOST);
    let mut available_ports = AvailablePorts::new(TestIdentifier::HttpServerUnitTests.into(), 3);
    let http_server_config = HttpServerConfig { ip, port: available_ports.get_next_port() };
    let add_tx_http_client =
        http_client_server_setup(mock_gateway_client, http_server_config).await;

    // Test a successful response.
    let rpc_tx = invoke_tx(CairoVersion::default());
    let tx_hash = add_tx_http_client.assert_add_tx_success(rpc_tx).await;
    assert_eq!(tx_hash, expected_tx_hash);

    // Test a failed response.
    let rpc_tx = invoke_tx(CairoVersion::default());
    let panicking_task = AssertUnwindSafe(add_tx_http_client.assert_add_tx_success(rpc_tx));
    let error = panicking_task.catch_unwind().await.unwrap_err().downcast::<String>().unwrap();
    let error_str = format!("{}", error);
    assert_eq!(error_str, expected_err_str);
}
