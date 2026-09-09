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

    let (status, body, _headers) = read_response(response).await;
    assert_eq!(status, StatusCode::OK);
    let body_text = String::from_utf8(body).unwrap();
    assert!(
        body_text.contains("prover_build_info"),
        "scrape should include build_info, got:\n{body_text}"
    );
    // Don't assert on specific label values. `shared_handle` uses generic test
    // labels and other tests call it too, so checking that the build_info
    // series exists at all is enough.
    assert!(body_text.contains("version="));
    assert!(body_text.contains("git_sha="));
}
