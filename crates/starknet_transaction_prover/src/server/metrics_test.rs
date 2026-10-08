use http::{Method, StatusCode};
use tower::{Layer, ServiceExt};

use crate::server::metrics::{MetricsLayer, METRICS_PATH};
use crate::server::middleware_test_utils::{empty_request, fallthrough_service, read_response};
use crate::server::test_recorder::shared_handle;

#[tokio::test]
async fn get_metrics_renders_prometheus_text() {
    let handle = shared_handle().clone();
    let svc = MetricsLayer::new(handle).layer(fallthrough_service());

    let response = svc.oneshot(empty_request(Method::GET, METRICS_PATH)).await.unwrap();

    let (status, body, headers) = read_response(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(http::header::CONTENT_TYPE).unwrap(), "text/plain; version=0.0.4");
    let body_text = String::from_utf8(body).unwrap();
    let expected_build_info = "prover_build_info{version=\"0.0.0-test\",git_sha=\"test-sha\"} 1";
    assert!(body_text.lines().any(|line| line == expected_build_info));
}

#[tokio::test]
async fn non_get_metrics_falls_through() {
    let svc = MetricsLayer::new(shared_handle().clone()).layer(fallthrough_service());

    let response = svc.oneshot(empty_request(Method::POST, METRICS_PATH)).await.unwrap();

    let (status, _body, _headers) = read_response(response).await;
    assert_eq!(status, StatusCode::IM_A_TEAPOT);
}

#[tokio::test]
async fn get_other_path_falls_through() {
    let svc = MetricsLayer::new(shared_handle().clone()).layer(fallthrough_service());

    let response = svc.oneshot(empty_request(Method::GET, "/")).await.unwrap();

    let (status, _body, _headers) = read_response(response).await;
    assert_eq!(status, StatusCode::IM_A_TEAPOT);
}
