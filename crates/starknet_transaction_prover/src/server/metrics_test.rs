use http::{Method, StatusCode};
use tower::{Layer, ServiceExt};

use crate::server::metrics::{names, MetricsLayer, METRICS_PATH};
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

/// Guards the bucket configuration whose rationale is on `DURATION_HISTOGRAM_BUCKETS`.
#[tokio::test]
async fn duration_metrics_render_as_histograms_not_summaries() {
    let handle = shared_handle();
    metrics::histogram!(names::PROVE_TRANSACTION_DURATION_SECONDS, "outcome" => "success")
        .record(0.5);

    let scrape = handle.render();

    let name = names::PROVE_TRANSACTION_DURATION_SECONDS;
    assert!(
        scrape.contains(&format!("# TYPE {name} histogram")),
        "{name} must be exported as a histogram, got:\n{scrape}"
    );
    assert!(
        scrape.contains(&format!("{name}_bucket")),
        "{name} must expose _bucket series for histogram_quantile(), got:\n{scrape}"
    );
    assert!(
        !scrape.contains(&format!("# TYPE {name} summary")),
        "{name} must not fall back to a rolling-window summary, got:\n{scrape}"
    );
}
