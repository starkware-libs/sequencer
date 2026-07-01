//! Unit tests for [`HttpMetricsLayer`].
//!
//! All HTTP-metric tests live in this single `#[tokio::test]` because the
//! Prometheus recorder is process-global: parallel tests sharing the same
//! recorder would race on counter values. We run a sequence of requests
//! and assert deltas between them.

use bytes::Bytes;
use http::{Method, Request, Response, StatusCode};
use http_body_util::Full;
use jsonrpsee::server::HttpBody;
use tower::{Layer, ServiceExt};

use crate::server::http_metrics::{names, HttpMetricsLayer};
use crate::server::test_recorder::shared_handle;

fn ok_service() -> impl tower::Service<
    Request<HttpBody>,
    Response = Response<HttpBody>,
    Error = std::convert::Infallible,
    Future = futures::future::Ready<Result<Response<HttpBody>, std::convert::Infallible>>,
> + Clone {
    tower::service_fn(|_req: Request<HttpBody>| {
        let response = Response::builder()
            .status(StatusCode::OK)
            .body(HttpBody::new(Full::new(Bytes::new())))
            .expect("static body is infallible");
        futures::future::ready(Ok::<_, std::convert::Infallible>(response))
    })
}

fn build_request(method: Method) -> Request<HttpBody> {
    Request::builder()
        .method(method)
        .uri("/")
        .body(HttpBody::new(Full::new(Bytes::new())))
        .expect("static body is infallible")
}

#[tokio::test]
async fn records_counter_histogram_and_returns_inflight_to_zero() {
    let handle = shared_handle();
    let svc = HttpMetricsLayer.layer(ok_service());

    // Capture counter / histogram baselines before this test runs so we
    // can assert deltas — the recorder is shared across the test binary
    // so other tests may have moved the absolute values.
    let before = parse_counter_and_histogram(&handle.render());

    // Issue three POSTs sequentially. After each await the in-flight gauge
    // must drop back to zero (the guard runs on future completion).
    for _ in 0..3 {
        let response = svc.clone().oneshot(build_request(Method::POST)).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    let scrape = handle.render();
    let after = parse_counter_and_histogram(&scrape);
    assert_eq!(after.counter - before.counter, 3.0, "counter delta");
    assert_eq!(after.histogram_count - before.histogram_count, 3.0, "histogram delta");

    // Gauge returned to zero — guard ran for every request.
    let gauge_line = scrape
        .lines()
        .find(|line| line.starts_with(names::IN_FLIGHT_REQUESTS) && !line.starts_with("# "))
        .unwrap_or_else(|| panic!("missing in-flight gauge in scrape:\n{scrape}"));
    let gauge_value: f64 =
        gauge_line.rsplit_once(' ').and_then(|(_, value)| value.parse().ok()).expect("gauge parse");
    assert_eq!(gauge_value, 0.0);
}

struct Snapshot {
    counter: f64,
    histogram_count: f64,
}

fn parse_counter_and_histogram(scrape: &str) -> Snapshot {
    let counter = scrape
        .lines()
        .find(|line| {
            line.starts_with(names::REQUESTS_TOTAL)
                && line.contains("method=\"POST\"")
                && line.contains("status=\"2xx\"")
                && !line.starts_with("# ")
        })
        .and_then(|line| line.rsplit_once(' ').and_then(|(_, value)| value.parse().ok()))
        .unwrap_or(0.0);
    let histogram_count = scrape
        .lines()
        .find(|line| {
            line.starts_with(&format!("{}_count", names::REQUEST_DURATION_SECONDS))
                && line.contains("method=\"POST\"")
                && !line.starts_with("# ")
        })
        .and_then(|line| line.rsplit_once(' ').and_then(|(_, value)| value.parse().ok()))
        .unwrap_or(0.0);
    Snapshot { counter, histogram_count }
}
