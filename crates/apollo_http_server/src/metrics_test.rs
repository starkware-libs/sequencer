use std::net::{IpAddr, Ipv4Addr};

use apollo_gateway_types::communication::{GatewayClientError, MockGatewayClient};
use apollo_gateway_types::gateway_types::{GatewayOutput, InvokeGatewayOutput};
use apollo_infra::component_client::ClientError;
use apollo_infra_utils::test_utils::{AvailablePorts, TestIdentifier};
use blockifier_test_utils::cairo_versions::CairoVersion;
use mempool_test_utils::starknet_api_test_utils::invoke_tx;
use metrics_exporter_prometheus::PrometheusBuilder;
use starknet_api::transaction::TransactionHash;

use crate::config::HttpServerConfig;
use crate::metrics::{
    ADDED_TRANSACTIONS_FAILURE,
    ADDED_TRANSACTIONS_SUCCESS,
    ADDED_TRANSACTIONS_TOTAL,
};
use crate::test_utils::http_client_server_setup;

#[tokio::test]
async fn get_metrics_test() {
    // Create a mock gateway client that returns a successful response and a failure response.
    const SUCCESS_TXS_TO_SEND: usize = 1;
    const FAILURE_TXS_TO_SEND: usize = 1;

    let mut mock_gateway_client = MockGatewayClient::new();
    // Set the successful response.
    mock_gateway_client.expect_add_tx().times(1).return_once(move |_| {
        Ok(GatewayOutput::Invoke(InvokeGatewayOutput::new(TransactionHash::default())))
    });
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
    let http_server_config = HttpServerConfig { ip, port: available_ports.get_next_port() };
    let add_tx_http_client =
        http_client_server_setup(mock_gateway_client, http_server_config).await;

    // Send transactions to the server.
    for _ in std::iter::repeat(()).take(SUCCESS_TXS_TO_SEND + FAILURE_TXS_TO_SEND) {
        let rpc_tx = invoke_tx(CairoVersion::default());
        add_tx_http_client.add_tx(rpc_tx).await;
    }

    // Obtain and parse metrics.
    let metrics = prometheus_handle.render();

    // Ensure the metric values are as expected.
    ADDED_TRANSACTIONS_TOTAL
        .assert_eq::<usize>(&metrics, SUCCESS_TXS_TO_SEND + FAILURE_TXS_TO_SEND);
    ADDED_TRANSACTIONS_SUCCESS.assert_eq::<usize>(&metrics, SUCCESS_TXS_TO_SEND);
    ADDED_TRANSACTIONS_FAILURE.assert_eq::<usize>(&metrics, FAILURE_TXS_TO_SEND);
}
