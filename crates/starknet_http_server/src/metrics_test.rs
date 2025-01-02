use std::net::SocketAddr;
use std::sync::Arc;

use blockifier::test_utils::CairoVersion;
use infra_utils::metrics::parse_numeric_metric;
use mempool_test_utils::starknet_api_test_utils::invoke_tx;
use metrics_exporter_prometheus::PrometheusBuilder;
use starknet_api::transaction::TransactionHash;
use starknet_gateway_types::communication::{GatewayClientError, MockGatewayClient};
use starknet_sequencer_infra::component_client::ClientError;
use tokio::task;

use crate::config::HttpServerConfig;
use crate::http_server::HttpServer;
use crate::metrics::{
    ADDED_TRANSACTIONS_FAILURE,
    ADDED_TRANSACTIONS_SUCCESS,
    ADDED_TRANSACTIONS_TOTAL,
};
use crate::test_utils::HttpTestClient;

#[tokio::test]
async fn get_metrics_test() {
    // Create a mock gateway client that returns a successful response and a failure response.
    const SUCCESS_TXS_TO_SEND: usize = 1;
    const FAILURE_TXS_TO_SEND: usize = 1;

    let mut mock_gateway_client = MockGatewayClient::new();
    // Set the successful response.
    mock_gateway_client
        .expect_add_tx()
        .times(1)
        .return_once(move |_| Ok(TransactionHash::default()));
    // Set the failure response.
    mock_gateway_client.expect_add_tx().times(1).return_once(move |_| {
        Err(GatewayClientError::ClientError(ClientError::UnexpectedResponse(
            "mock response".to_string(),
        )))
    });

    // Initialize the metrics directly instead of spawning a monitoring endpoint task.
    let prometheus_handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("should be able to build the recorder and install it globally");

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
    for _ in std::iter::repeat(()).take(SUCCESS_TXS_TO_SEND + FAILURE_TXS_TO_SEND) {
        let rpc_tx = invoke_tx(CairoVersion::default());
        add_tx_http_client.add_tx(rpc_tx).await;
    }

    // Obtain and parse metrics.
    let metrics = prometheus_handle.render();
    let added_transactions_total_count =
        parse_numeric_metric::<usize>(&metrics, ADDED_TRANSACTIONS_TOTAL.0);
    let added_transactions_success_count =
        parse_numeric_metric::<usize>(&metrics, ADDED_TRANSACTIONS_SUCCESS.0);
    let added_transactions_failure_count =
        parse_numeric_metric::<usize>(&metrics, ADDED_TRANSACTIONS_FAILURE.0);

    // Ensure the metric values are as expected.
    assert_eq!(
        added_transactions_total_count.unwrap(),
        SUCCESS_TXS_TO_SEND + FAILURE_TXS_TO_SEND,
        "Total transaction count mismatch"
    );
    assert_eq!(
        added_transactions_success_count.unwrap(),
        SUCCESS_TXS_TO_SEND,
        "Successful transaction count mismatch"
    );
    assert_eq!(
        added_transactions_failure_count.unwrap(),
        FAILURE_TXS_TO_SEND,
        "Failing transaction count mismatch"
    );
}
