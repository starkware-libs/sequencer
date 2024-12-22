use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::{Bytes, HttpBody};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use blockifier::test_utils::CairoVersion;
use infra_utils::metrics::parse_numeric_metric;
use mempool_test_utils::starknet_api_test_utils::invoke_tx;
use metrics_exporter_prometheus::PrometheusBuilder;
use starknet_api::transaction::TransactionHash;
use starknet_gateway_types::communication::{GatewayClientError, MockGatewayClient};
use starknet_sequencer_infra::component_client::ClientError;
use tokio::task;

use crate::config::HttpServerConfig;
use crate::http_server::{add_tx_result_as_json, HttpServer};
use crate::metrics::{
    ADDED_TRANSACTIONS_FAILURE,
    ADDED_TRANSACTIONS_SUCCESS,
    ADDED_TRANSACTIONS_TOTAL,
};
use crate::test_utils::HttpTestClient;

// TODO(Tsabary): fix the toml file.
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
async fn get_metrics_test() {
    let prometheus_handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("should be able to build the recorder and install it globally");

    // TODO(Tsabary): there is a bug in the http server where a failed gateway request crashes the
    // server. The current setup does not test that. Fix the bug and update this test accordingly.
    // Create a mock gateway client that returns arbitrary responses.
    let txs_to_send = 3;
    let success_txs_to_send = txs_to_send;
    let failure_txs_to_send = txs_to_send - success_txs_to_send;
    let mut mock_gateway_client = MockGatewayClient::new();
    let mut i = 0;
    mock_gateway_client.expect_add_tx().times(txs_to_send).returning(move |_| {
        i += 1;
        match i {
            0 => Err(GatewayClientError::ClientError(ClientError::UnexpectedResponse(
                "mock response".to_string(),
            ))),
            1 => Ok(TransactionHash::default()),
            _ => Ok(TransactionHash::default()),
        }
    });

    // TODO(Tsabary): replace the const port with something that is not hardcoded.
    // Create and run the server.
    let http_server_config = HttpServerConfig { ip: "127.0.0.1".parse().unwrap(), port: 15123 };
    let mut http_server =
        HttpServer::new(http_server_config.clone(), Arc::new(mock_gateway_client));
    tokio::spawn(async move { http_server.run().await });

    let HttpServerConfig { ip, port } = http_server_config;
    let add_tx_http_client = HttpTestClient::new(SocketAddr::from((ip, port)));

    // Ensure the server starts running.
    task::yield_now().await;

    // Send transactions to the server.
    for _ in std::iter::repeat(()).take(txs_to_send) {
        let rpc_tx = invoke_tx(CairoVersion::default());
        add_tx_http_client.add_tx(rpc_tx).await;
    }

    // Obtain metrics from the server.
    let added_transactions_total_count =
        parse_numeric_metric::<usize>(&prometheus_handle, ADDED_TRANSACTIONS_SUCCESS.0);
    let added_transactions_success_count =
        parse_numeric_metric::<usize>(&prometheus_handle, ADDED_TRANSACTIONS_TOTAL.0);
    let added_transactions_failure_count =
        parse_numeric_metric::<usize>(&prometheus_handle, ADDED_TRANSACTIONS_FAILURE.0);

    // Ensure the metrics are as expected.
    assert_eq!(added_transactions_total_count.unwrap(), txs_to_send);
    assert_eq!(added_transactions_success_count.unwrap(), success_txs_to_send);
    assert_eq!(added_transactions_failure_count.unwrap(), failure_txs_to_send);
}
