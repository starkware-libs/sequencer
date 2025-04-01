use std::net::{IpAddr, Ipv4Addr};

use apollo_gateway_types::communication::{GatewayClientError, MockGatewayClient};
use apollo_infra_utils::test_utils::{AvailablePorts, TestIdentifier};
use apollo_sequencer_infra::component_client::ClientError;
use metrics_exporter_prometheus::PrometheusBuilder;
use rstest::rstest;
use serde::Serialize;
use starknet_api::transaction::TransactionHash;

use crate::config::HttpServerConfig;
use crate::metrics::{
    ADDED_TRANSACTIONS_FAILURE,
    ADDED_TRANSACTIONS_SUCCESS,
    ADDED_TRANSACTIONS_TOTAL,
};
use crate::test_utils::{http_client_server_setup, rest_tx, rpc_tx, HttpServerEndpoint};

#[rstest]
#[case::add_rest_tx(0, HttpServerEndpoint::AddTx, rest_tx())]
#[case::add_rpc_tx(1, HttpServerEndpoint::AddRpcTx, rpc_tx())]
#[tokio::test]
async fn add_tx_metrics_test(
    #[case] index: u16,
    #[case] endpoint: HttpServerEndpoint,
    #[case] tx: impl Serialize + Clone,
) {
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
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    let prometheus_handle = recorder.handle();

    let ip = IpAddr::from(Ipv4Addr::LOCALHOST);
    let mut available_ports = AvailablePorts::new(TestIdentifier::HttpServerUnitTests.into(), 0);
    let http_server_config = HttpServerConfig { ip, port: available_ports.get_next_port() + index };
    let add_tx_http_client =
        http_client_server_setup(mock_gateway_client, http_server_config).await;

    // Send transactions to the server.
    for _ in std::iter::repeat(()).take(SUCCESS_TXS_TO_SEND + FAILURE_TXS_TO_SEND) {
        add_tx_http_client.add_tx(tx.clone(), endpoint).await;
    }

    // Obtain and parse metrics.
    let metrics = prometheus_handle.render();

    // Ensure the metric values are as expected.
    ADDED_TRANSACTIONS_TOTAL
        .assert_eq::<usize>(&metrics, SUCCESS_TXS_TO_SEND + FAILURE_TXS_TO_SEND);
    ADDED_TRANSACTIONS_SUCCESS.assert_eq::<usize>(&metrics, SUCCESS_TXS_TO_SEND);
    ADDED_TRANSACTIONS_FAILURE.assert_eq::<usize>(&metrics, FAILURE_TXS_TO_SEND);
}

#[tokio::test]
async fn add_tx_serde_failure_metrics_test() {
    let mut mock_gateway_client = MockGatewayClient::new();
    // Set the successful response.
    mock_gateway_client
        .expect_add_tx()
        .times(1)
        .return_once(move |_| Ok(TransactionHash::default()));

    // Initialize the metrics directly instead of spawning a monitoring endpoint task.
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    let prometheus_handle = recorder.handle();

    // TODO(Yael): share the creation of add_tx_http_client code for all tests in test_utils.
    let ip = IpAddr::from(Ipv4Addr::LOCALHOST);
    let mut available_ports = AvailablePorts::new(TestIdentifier::HttpServerUnitTests.into(), 0);
    let http_server_config = HttpServerConfig { ip, port: available_ports.get_next_port() + 2 };
    let add_tx_http_client =
        http_client_server_setup(mock_gateway_client, http_server_config).await;

    // Send a transaction that fails deserialization.
    let tx = "invalid_tx";
    add_tx_http_client.add_tx(tx, HttpServerEndpoint::AddTx).await;

    // Obtain and parse metrics.
    let metrics = prometheus_handle.render();

    // Ensure the metric values are as expected.
    ADDED_TRANSACTIONS_TOTAL.assert_eq::<usize>(&metrics, 1);
    ADDED_TRANSACTIONS_SUCCESS.assert_eq::<usize>(&metrics, 0);
    ADDED_TRANSACTIONS_FAILURE.assert_eq::<usize>(&metrics, 1);
}
