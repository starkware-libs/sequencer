use std::net::IpAddr;
use std::sync::Arc;

use axum::http::StatusCode;
use axum::response::Response;
use axum::Router;
use hyper::body::to_bytes;
use hyper::Client;
use metrics::{counter, describe_counter};
use pretty_assertions::assert_eq;
use starknet_api::tx_hash;
use starknet_mempool_types::communication::{MempoolClientResult, MockMempoolClient};
use starknet_mempool_types::mempool_types::MempoolSnapshot;
use tokio::spawn;
use tokio::task::yield_now;
use tower::ServiceExt;

use super::MonitoringEndpointConfig;
use crate::config::{DEFAULT_IP, DEFAULT_PORT};
use crate::monitoring_endpoint::{
    create_monitoring_endpoint,
    MonitoringEndpoint,
    ALIVE,
    MEMPOOL_SNAPSHOT,
    METRICS,
    READY,
    VERSION,
};
use crate::test_utils::build_request;

const TEST_VERSION: &str = "1.2.3-dev";

// Note: the metrics recorder is installed globally, causing tests to conflict when run in parallel.
// Most tests do not require it, and as such, use the following disabling config.
const CONFIG_WITHOUT_METRICS: MonitoringEndpointConfig = MonitoringEndpointConfig {
    ip: DEFAULT_IP,
    port: DEFAULT_PORT,
    collect_metrics: false,
    collect_profiling_metrics: false,
};

fn setup_monitoring_endpoint(config: Option<MonitoringEndpointConfig>) -> MonitoringEndpoint {
    let config = config.unwrap_or(CONFIG_WITHOUT_METRICS);
    let mut mock_mempool_client = MockMempoolClient::new();
    mock_mempool_client.expect_get_mempool_snapshot().returning(return_mempool_snapshot);
    let shared_mock_mempool_client = Arc::new(mock_mempool_client);

    create_monitoring_endpoint(config, TEST_VERSION, shared_mock_mempool_client)
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

fn return_mempool_snapshot() -> MempoolClientResult<MempoolSnapshot> {
    let expected_chronological_hashes = (1..10).map(|i| tx_hash!(i)).collect::<Vec<_>>();
    Ok(MempoolSnapshot { transactions: expected_chronological_hashes })
}

#[tokio::test]
async fn mempool_snapshot() {
    let config = MonitoringEndpointConfig { collect_metrics: false, ..Default::default() };
    let app = setup_monitoring_endpoint(Some(config)).app();

    let response = request_app(app, MEMPOOL_SNAPSHOT).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let body_string = String::from_utf8(body_bytes.to_vec()).unwrap();
    let expected_prefix =
        String::from(r#"{"transactions":["0x1","0x2","0x3","0x4","0x5","0x6","0x7","0x8","0x9"]}"#);

    assert!(body_string.starts_with(&expected_prefix));
}
