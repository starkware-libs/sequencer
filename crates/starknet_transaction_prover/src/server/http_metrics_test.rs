//! Unit tests for [`HttpMetricsLayer`].
//!
//! Each test installs its own local Prometheus recorder, so a scrape holds only the samples that
//! test recorded and every assertion is exact. A local recorder is thread-local, so the tests run
//! on a current-thread runtime and hold the recorder guard across the awaited requests and the
//! scrape.

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
use crate::server::metrics::{configured_builder, MetricsLayer, METRICS_PATH};
use crate::server::request_log::RequestLogLayer;
use crate::server::request_span::RequestSpanLayer;
use crate::server::test_recorder::metric_value;
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

fn build_request(method: Method, path: &str) -> Request<HttpBody> {
    Request::builder()
        .method(method)
        .uri(path)
        .body(HttpBody::new(Full::new(Bytes::new())))
        .expect("static body is infallible")
}

fn post_2xx_counter_line() -> String {
    format!("{}{{method=\"POST\",status=\"2xx\"}}", names::REQUESTS_TOTAL)
}

fn post_duration_count_line() -> String {
    format!("{}_count{{method=\"POST\",status=\"2xx\"}}", names::REQUEST_DURATION_SECONDS)
}

/// Sample lines of `prover_http_requests_total` carrying `method="GET"`, across statuses.
fn get_request_samples(scrape: &str) -> Vec<&str> {
    scrape
        .lines()
        .filter(|line| !line.starts_with('#') && line.starts_with(names::REQUESTS_TOTAL))
        .filter(|line| line.contains("method=\"GET\""))
        .collect()
}

#[tokio::test(flavor = "current_thread")]
async fn records_counter_histogram_and_returns_inflight_to_zero() {
    let recorder = configured_builder().unwrap().build_recorder();
    let handle = recorder.handle();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    let svc = HttpMetricsLayer.layer(ok_service());

    for _ in 0..3 {
        let response = svc.clone().oneshot(build_request(Method::POST, "/")).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    let scrape = handle.render();
    assert_eq!(metric_value(&scrape, &post_2xx_counter_line()), 3.0, "request counter");
    assert_eq!(
        metric_value(&scrape, &post_duration_count_line()),
        3.0,
        "latency histogram count, on the series carrying both method and status labels"
    );
    // The gauge is back at zero, so the guard ran for every request.
    assert_eq!(metric_value(&scrape, names::IN_FLIGHT_REQUESTS), 0.0);
}

/// `HttpMetricsLayer` sits below `HealthLayer` and `MetricsLayer` in the
/// production chain so probe and scrape traffic stays out of the request
/// distribution. Layer order is the only thing enforcing that, so this test
/// fails if the order changes. The trailing POST proves the layer is in the
/// chain at all, so an empty scrape cannot pass the test by accident.
#[tokio::test(flavor = "current_thread")]
async fn probe_and_scrape_traffic_is_excluded_from_http_metrics() {
    let recorder = configured_builder().unwrap().build_recorder();
    let handle = recorder.handle();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    let svc = prover_http_middleware!(
        MetricsLayer::new(handle.clone()),
        None::<CorsLayer>,
        None::<OhttpJsonrpseeLayer>,
    )
    .service(ok_service());

    for path in [HEALTH_PATH, METRICS_PATH] {
        let response = svc.clone().oneshot(build_request(Method::GET, path)).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{path} should be served by its layer");
    }
    let response = svc.clone().oneshot(build_request(Method::POST, "/")).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let scrape = handle.render();
    assert_eq!(metric_value(&scrape, &post_2xx_counter_line()), 1.0, "the POST reached the layer");
    assert!(
        get_request_samples(&scrape).is_empty(),
        "short-circuited {HEALTH_PATH}/{METRICS_PATH} traffic must not reach the HTTP metrics \
         layer; scrape:\n{scrape}"
    );
}
