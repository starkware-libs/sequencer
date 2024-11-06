use std::net::IpAddr;

use axum::body::Bytes;
use axum::http::StatusCode;
use axum::response::Response;
use axum::{Error, Router};
use http_body::combinators::UnsyncBoxBody;
use hyper::body::to_bytes;
use hyper::Client;
use pretty_assertions::assert_eq;
use tokio::spawn;
use tokio::task::yield_now;
use tower::ServiceExt;

use super::MonitoringEndpointConfig;
use crate::monitoring_endpoint::{
    create_monitoring_endpoint,
    MonitoringEndpoint,
    ALIVE,
    READY,
    VERSION,
};
use crate::test_utils::build_request;

// TODO(Tsabary): Clear feature dependencies and dev dependencies.

const TEST_VERSION: &str = "1.2.3-dev";

fn setup_monitoring_endpoint() -> MonitoringEndpoint {
    create_monitoring_endpoint(MonitoringEndpointConfig::default(), TEST_VERSION)
}

async fn request_app(app: Router, method: &str) -> Response<UnsyncBoxBody<Bytes, Error>> {
    app.oneshot(build_request(&IpAddr::from([0, 0, 0, 0]), 0, method)).await.unwrap()
}

#[tokio::test]
async fn test_node_version() {
    let response = request_app(setup_monitoring_endpoint().app(), VERSION).await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body()).await.unwrap();
    assert_eq!(&body[..], TEST_VERSION.as_bytes());
}

#[tokio::test]
async fn test_alive() {
    let response = request_app(setup_monitoring_endpoint().app(), ALIVE).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_ready() {
    let response = request_app(setup_monitoring_endpoint().app(), READY).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_endpoint_as_server() {
    spawn(async move { setup_monitoring_endpoint().run().await });
    yield_now().await;

    let MonitoringEndpointConfig { ip, port } = MonitoringEndpointConfig::default();

    let client = Client::new();

    let response = client.request(build_request(&ip, port, VERSION)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body()).await.unwrap();
    assert_eq!(&body[..], TEST_VERSION.as_bytes());
}
