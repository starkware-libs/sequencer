use http::{Method, StatusCode};
use tower::{Layer, ServiceExt};

use crate::server::metrics::{configured_builder, names, MetricsLayer, METRICS_PATH};
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

#[test]
fn duration_metrics_render_as_histograms_not_summaries() {
    let recorder = configured_builder().unwrap().build_recorder();
    let handle = recorder.handle();
    metrics::with_local_recorder(&recorder, || {
        metrics::histogram!(names::PROVE_TRANSACTION_DURATION_SECONDS, "outcome" => "success")
            .record(0.5);
        metrics::histogram!(names::OS_RUN_DURATION_SECONDS).record(0.5);
        metrics::histogram!(names::STWO_PROVE_DURATION_SECONDS).record(0.5);
    });

    let scrape = handle.render();

    for name in [
        names::PROVE_TRANSACTION_DURATION_SECONDS,
        names::OS_RUN_DURATION_SECONDS,
        names::STWO_PROVE_DURATION_SECONDS,
    ] {
        assert!(scrape.contains(&format!("# TYPE {name} histogram")), "{name}: {scrape}");
        assert!(scrape.contains(&format!("{name}_bucket")), "{name}: missing buckets");
        assert!(!scrape.contains(&format!("# TYPE {name} summary")), "{name}: summary");
    }
}
