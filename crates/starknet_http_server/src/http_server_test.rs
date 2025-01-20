use axum::body::{Bytes, HttpBody};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use blockifier::test_utils::CairoVersion;
use mempool_test_utils::starknet_api_test_utils::invoke_tx;
use starknet_api::transaction::TransactionHash;
use starknet_gateway_types::communication::{GatewayClientError, MockGatewayClient};
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
    mock_gateway_client.expect_add_tx().times(1).return_const(Ok(tx_hash_1));
    mock_gateway_client.expect_add_tx().times(1).return_const(Ok(tx_hash_2));

    let ip = "127.0.0.1".parse().unwrap();
    // TODO(Tsabary): replace the const port with something that is not hardcoded.
    let port = 15124;
    let http_server_config = HttpServerConfig { ip, port };
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
    mock_gateway_client.expect_add_tx().times(1).return_const(Err(
        GatewayClientError::ClientError(ClientError::UnexpectedResponse(
            "mock response".to_string(),
        )),
    ));

    let ip = "127.0.0.1".parse().unwrap();
    // TODO(Tsabary): replace the const port with something that is not hardcoded.
    let port = 15125;
    let http_server_config = HttpServerConfig { ip, port };
    let add_tx_http_client =
        http_client_server_setup(mock_gateway_client, http_server_config).await;

    // Send a transaction to the server.
    let rpc_tx = invoke_tx(CairoVersion::default());
    add_tx_http_client.add_tx(rpc_tx).await;
    assert!(!logs_contain("Recorded transaction with hash: "));
}
