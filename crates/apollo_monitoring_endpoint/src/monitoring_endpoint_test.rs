use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use apollo_l1_provider_types::{L1ProviderSnapshot, MockL1ProviderClient};
use apollo_mempool_types::communication::MockMempoolClient;
use apollo_mempool_types::mempool_types::{
    MempoolSnapshot,
    MempoolStateSnapshot,
    TransactionQueueSnapshot,
};
use axum::http::StatusCode;
use axum::response::Response;
use axum::Router;
use hyper::body::to_bytes;
use hyper::Client;
use metrics::{counter, describe_counter};
use pretty_assertions::assert_eq;
use serde_json::{from_slice, to_value, Value};
use starknet_api::block::{BlockNumber, GasPrice};
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::{nonce, tx_hash};
use tokio::spawn;
use tokio::task::yield_now;
use tower::ServiceExt;

use super::MonitoringEndpointConfig;
use crate::config::{MONITORING_ENDPOINT_DEFAULT_IP, MONITORING_ENDPOINT_DEFAULT_PORT};
use crate::monitoring_endpoint::{
    create_monitoring_endpoint,
    MonitoringEndpoint,
    ALIVE,
    L1_PROVIDER_SNAPSHOT,
    MEMPOOL_SNAPSHOT,
    METRICS,
    READY,
    VERSION,
};
use crate::test_utils::build_request;
use crate::tokio_metrics::{
    TOKIO_GLOBAL_QUEUE_DEPTH,
    TOKIO_MAX_BUSY_DURATION,
    TOKIO_MAX_PARK_COUNT,
    TOKIO_MIN_BUSY_DURATION,
    TOKIO_MIN_PARK_COUNT,
    TOKIO_TOTAL_BUSY_DURATION,
    TOKIO_TOTAL_PARK_COUNT,
    TOKIO_WORKERS_COUNT,
};

const TEST_VERSION: &str = "1.2.3-dev";

// Note: the metrics recorder is installed globally, causing tests to conflict when run in parallel.
// Most tests do not require it, and as such, use the following disabling config.
const CONFIG_WITHOUT_METRICS: MonitoringEndpointConfig = MonitoringEndpointConfig {
    ip: MONITORING_ENDPOINT_DEFAULT_IP,
    port: MONITORING_ENDPOINT_DEFAULT_PORT,
    collect_metrics: false,
    collect_profiling_metrics: false,
};

fn setup_monitoring_endpoint(config: Option<MonitoringEndpointConfig>) -> MonitoringEndpoint {
    let config = config.unwrap_or(CONFIG_WITHOUT_METRICS);
    create_monitoring_endpoint(config, TEST_VERSION, None, None)
}

async fn request_app(app: Router, method: &str) -> Response {
    app.oneshot(build_request(&IpAddr::from([0, 0, 0, 0]), 0, method)).await.unwrap()
}

#[tokio::test]
async fn node_version() {
    let response = request_app(setup_monitoring_endpoint(None).app(), VERSION).await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body()).await.unwrap();
    assert_eq!(&body[..], TEST_VERSION.as_bytes());
}

#[tokio::test]
async fn alive_endpoint() {
    let response = request_app(setup_monitoring_endpoint(None).app(), ALIVE).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn ready_endpoint() {
    let response = request_app(setup_monitoring_endpoint(None).app(), READY).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn with_metrics() {
    let config = MonitoringEndpointConfig { collect_metrics: true, ..Default::default() };
    let app = setup_monitoring_endpoint(Some(config)).app();

    // Register a metric.
    let metric_name = "metric_name";
    let metric_help = "metric_help";
    let metric_value = 8224;
    counter!(metric_name).absolute(metric_value);
    describe_counter!(metric_name, metric_help);
    let response = request_app(app, METRICS).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let body_string = String::from_utf8(body_bytes.to_vec()).unwrap();
    let expected_prefix = format!(
        "# HELP {metric_name} {metric_help}\n# TYPE {metric_name} counter\n{metric_name} \
         {metric_value}\n\n"
    );
    assert!(body_string.starts_with(&expected_prefix));
}

#[tokio::test]
async fn without_metrics() {
    let app = setup_monitoring_endpoint(None).app();
    let response = request_app(app, METRICS).await;
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn endpoint_as_server() {
    spawn(async move { setup_monitoring_endpoint(None).run().await });
    yield_now().await;

    let MonitoringEndpointConfig { ip, port, .. } = MonitoringEndpointConfig::default();

    let client = Client::new();

    let response = client.request(build_request(&ip, port, VERSION)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body()).await.unwrap();
    assert_eq!(&body[..], TEST_VERSION.as_bytes());
}

fn setup_monitoring_endpoint_with_mempool_client() -> MonitoringEndpoint {
    let mut mock_mempool_client = MockMempoolClient::new();
    mock_mempool_client.expect_get_mempool_snapshot().returning(|| Ok(expected_mempool_snapshot()));
    let shared_mock_mempool_client = Arc::new(mock_mempool_client);

    create_monitoring_endpoint(
        CONFIG_WITHOUT_METRICS,
        TEST_VERSION,
        Some(shared_mock_mempool_client),
        None,
    )
}

fn create_hash_map(i: u32, j: u32) -> HashMap<ContractAddress, Nonce> {
    let contract_addresses = (i..i + 2).map(ContractAddress::from).collect::<Vec<_>>();
    let nonces = (j..j + 2).map(|n| nonce!(n)).collect::<Vec<_>>();

    contract_addresses.into_iter().zip(nonces).collect()
}

fn expected_mempool_snapshot() -> MempoolSnapshot {
    let expected_chronological_hashes = (1..10).map(|i| tx_hash!(i)).collect::<Vec<_>>();
    let expected_delayed_declares = (10..15).map(|i| tx_hash!(i)).collect::<Vec<_>>();
    let expected_transaction_queue = TransactionQueueSnapshot {
        gas_price_threshold: GasPrice(1),
        priority_queue: (1..5).map(|i| tx_hash!(i)).collect::<Vec<_>>(),
        pending_queue: (5..10).map(|i| tx_hash!(i)).collect::<Vec<_>>(),
    };
    let mempool_state =
        MempoolStateSnapshot { committed: create_hash_map(1, 3), staged: create_hash_map(5, 7) };
    MempoolSnapshot {
        transactions: expected_chronological_hashes,
        delayed_declares: expected_delayed_declares,
        transaction_queue: expected_transaction_queue,
        mempool_state,
    }
}

#[tokio::test]
async fn mempool_snapshot() {
    let app = setup_monitoring_endpoint_with_mempool_client().app();

    let response = request_app(app, MEMPOOL_SNAPSHOT).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = hyper::body::to_bytes(response.into_body()).await.unwrap();

    let expected_json =
        to_value(expected_mempool_snapshot()).expect("Failed to serialize MempoolSnapshot");
    let received_json: Value = from_slice(&body_bytes).expect("Failed to parse JSON string");

    assert_eq!(expected_json, received_json);
}

#[tokio::test]
async fn mempool_not_present() {
    let app = setup_monitoring_endpoint(None).app();
    let response = request_app(app, MEMPOOL_SNAPSHOT).await;
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

fn setup_monitoring_endpoint_with_l1_provider_client() -> MonitoringEndpoint {
    let mut l1_provider_client = MockL1ProviderClient::new();
    l1_provider_client
        .expect_get_l1_provider_snapshot()
        .returning(|| Ok(expected_l1_provider_snapshot()));
    let shared_mock_l1_provider_client = Arc::new(l1_provider_client);

    create_monitoring_endpoint(
        CONFIG_WITHOUT_METRICS,
        TEST_VERSION,
        None,
        Some(shared_mock_l1_provider_client),
    )
}

fn expected_l1_provider_snapshot() -> L1ProviderSnapshot {
    let expected_uncommitted_hashes = (1..10).map(|i| tx_hash!(i)).collect::<Vec<_>>();
    let expected_uncommitted_staged_hashes = (1..2).map(|i| tx_hash!(i)).collect::<Vec<_>>();
    let expected_rejected_hashes = (10..15).map(|i| tx_hash!(i)).collect::<Vec<_>>();
    let expected_rejected_staged_hashes = (10..12).map(|i| tx_hash!(i)).collect::<Vec<_>>();
    let expected_committed_hashes = (15..20).map(|i| tx_hash!(i)).collect::<Vec<_>>();
    let l1_provider_state = String::from("Validate");
    let current_height = BlockNumber(1);
    L1ProviderSnapshot {
        uncommitted_transactions: expected_uncommitted_hashes,
        uncommitted_staged_transactions: expected_uncommitted_staged_hashes,
        rejected_transactions: expected_rejected_hashes,
        rejected_staged_transactions: expected_rejected_staged_hashes,
        committed_transactions: expected_committed_hashes,
        l1_provider_state,
        current_height,
    }
}

#[tokio::test]
async fn l1_provider_snapshot() {
    let app = setup_monitoring_endpoint_with_l1_provider_client().app();

    let response = request_app(app, L1_PROVIDER_SNAPSHOT).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = hyper::body::to_bytes(response.into_body()).await.unwrap();

    let expected_json = to_value(expected_l1_provider_snapshot()).expect(
        "Failed to serialize
L1ProviderSnapshot",
    );
    let received_json: Value = from_slice(&body_bytes).expect(
        "Failed to
parse JSON string",
    );

    assert_eq!(expected_json, received_json);
}

#[tokio::test]
async fn l1_provider_not_present() {
    let app = setup_monitoring_endpoint(None).app();
    let response = request_app(app, L1_PROVIDER_SNAPSHOT).await;
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn print_prometheus_metrics_output() {
    let config = MonitoringEndpointConfig { collect_metrics: true, ..CONFIG_WITHOUT_METRICS };
    let app = setup_monitoring_endpoint(Some(config)).app();

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
    })
    .await
    .unwrap();

    let response = request_app(app, METRICS).await;
    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let prometheus_output = String::from_utf8(body_bytes.to_vec()).unwrap();

    // Print the full Prometheus metrics output for debugging
    println!("=== PROMETHEUS METRICS OUTPUT ===");
    println!("{}", prometheus_output);
    println!("=== END PROMETHEUS METRICS OUTPUT ===");

    TOKIO_TOTAL_BUSY_DURATION.assert_eq(&prometheus_output, 0u64);
    TOKIO_MIN_BUSY_DURATION.assert_eq(&prometheus_output, 0u64);
    TOKIO_MAX_BUSY_DURATION.assert_eq(&prometheus_output, 0u64);
    TOKIO_TOTAL_PARK_COUNT.assert_eq(&prometheus_output, 0u64);
    TOKIO_MIN_PARK_COUNT.assert_eq(&prometheus_output, 0u64);
    TOKIO_MAX_PARK_COUNT.assert_eq(&prometheus_output, 0u64);
    TOKIO_WORKERS_COUNT.assert_eq(&prometheus_output, 1u64);
    TOKIO_GLOBAL_QUEUE_DEPTH.assert_eq(&prometheus_output, 0u64);
}
