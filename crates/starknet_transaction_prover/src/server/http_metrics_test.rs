//! Unit tests for [`HttpMetricsLayer`].
//!
//! All tests share one process-global Prometheus recorder ([`shared_handle`]),
//! so absolute sample values also reflect requests driven by other tests in this
//! binary; assertions take a baseline before the action and compare deltas.

use bytes::Bytes;
use http::{Method, Request, Response, StatusCode};
use http_body_util::Full;
use jsonrpsee::server::HttpBody;
use tower::{Layer, ServiceBuilder, ServiceExt};
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::map_request_body::MapRequestBodyLayer;
use tower_http::map_response_body::MapResponseBodyLayer;

use crate::server::health::{HealthLayer, HEALTH_PATH};
use crate::server::http_metrics::{names, HttpMetricsLayer};
use crate::server::metrics::{MetricsLayer, METRICS_PATH};
use crate::server::request_log::RequestLogLayer;
use crate::server::request_span::RequestSpanLayer;
use crate::server::test_recorder::{metric_value, shared_handle};
use crate::server::OhttpJsonrpseeLayer;

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

    let scrape = handle.render();
    let before_counter = metric_value(&scrape, &post_2xx_counter_line());
    let before_histogram = metric_value(&scrape, &post_duration_count_line());

    for _ in 0..3 {
        let response = svc.clone().oneshot(build_request(Method::POST)).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    let scrape = handle.render();
    assert_eq!(
        metric_value(&scrape, &post_2xx_counter_line()) - before_counter,
        3.0,
        "counter delta"
    );
    assert!(
        scrape.lines().any(|line| line.starts_with(&post_duration_count_line())),
        "latency histogram must carry both method and status labels; scrape:\n{scrape}"
    );
    assert_eq!(
        metric_value(&scrape, &post_duration_count_line()) - before_histogram,
        3.0,
        "histogram delta"
    );

    // Gauge returned to zero — guard ran for every request.
    assert_eq!(metric_value(&scrape, names::IN_FLIGHT_REQUESTS), 0.0);
}

/// Sums the `prover_http_requests_total` series carrying `method="GET"`, across
/// statuses. Scoped to GET because the recorder is process-global and other
/// tests in this binary drive POSTs through the same layer in parallel; only
/// this test sends a GET through it.
fn get_http_requests(scrape: &str) -> f64 {
    scrape
        .lines()
        .filter(|line| !line.starts_with('#') && line.starts_with(names::REQUESTS_TOTAL))
        .filter(|line| line.contains("method=\"GET\""))
        .filter_map(|line| line.rsplit_once(' ').and_then(|(_, value)| value.parse::<f64>().ok()))
        .sum()
}

fn post_2xx_counter_line() -> String {
    format!("{}{{method=\"POST\",status=\"2xx\"}}", names::REQUESTS_TOTAL)
}

fn post_duration_count_line() -> String {
    format!("{}_count{{method=\"POST\",status=\"2xx\"}}", names::REQUEST_DURATION_SECONDS)
}

/// `HttpMetricsLayer` sits below `HealthLayer`/`MetricsLayer` in the production
/// chain precisely so probe and scrape traffic stays out of the request
/// distribution. That exclusion was previously enforced by layer order alone,
/// with nothing failing if the order changed.
#[tokio::test]
async fn probe_and_scrape_traffic_is_excluded_from_http_metrics() {
    let handle = shared_handle();
    let svc = prover_http_middleware!(
        MetricsLayer::new(handle.clone()),
        None::<CorsLayer>,
        None::<OhttpJsonrpseeLayer>,
    )
    .service(ok_service());

    let before_get = get_http_requests(&handle.render());

    for path in [HEALTH_PATH, METRICS_PATH] {
        let request = Request::builder()
            .method(Method::GET)
            .uri(path)
            .body(HttpBody::new(Full::new(Bytes::new())))
            .expect("static body is infallible");
        let response = svc.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{path} should be served by its layer");
    }

    let after_get = get_http_requests(&handle.render());
    assert_eq!(
        after_get - before_get,
        0.0,
        "short-circuited {HEALTH_PATH}/{METRICS_PATH} traffic must not reach the HTTP metrics \
         layer"
    );
}
