//! In-test microbenchmarks for the new observability middleware.
//!
//! Run with:
//!     SEED=0 cargo test --release -p starknet_transaction_prover \
//!         server::overhead_bench -- --ignored --nocapture
//!
//! Marked `#[ignore]` so the regular `cargo test` run does not pay the
//! benchmark cost. The intent is to give an operator a quick way to
//! reproduce the per-request overhead numbers published in the
//! observability rollout report (see `observability_report.html`).

#![cfg(test)]

use std::time::Instant;

use bytes::Bytes;
use http::{Method, Request, Response, StatusCode};
use http_body_util::Full;
use jsonrpsee::server::HttpBody;
use tower::{Layer, ServiceExt};

use super::http_metrics::HttpMetricsLayer;
use super::test_recorder::shared_handle;

const ITERATIONS: usize = 50_000;

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

fn make_request() -> Request<HttpBody> {
    Request::builder()
        .method(Method::POST)
        .uri("/")
        .body(HttpBody::new(Full::new(Bytes::new())))
        .expect("static body is infallible")
}

async fn time_loop<S>(label: &str, svc: S) -> std::time::Duration
where
    S: tower::Service<Request<HttpBody>, Response = Response<HttpBody>> + Clone,
    S::Future: Send,
{
    // Warm-up: prime caches, exporter buffers, etc.
    for _ in 0..1_000 {
        let _ = svc.clone().oneshot(make_request()).await;
    }
    let start = Instant::now();
    for _ in 0..ITERATIONS {
        let _ = svc.clone().oneshot(make_request()).await;
    }
    let elapsed = start.elapsed();
    // `Duration / u32` is the only stable division API; safe to cast
    // because ITERATIONS is a tiny compile-time constant.
    let iterations_u32 = u32::try_from(ITERATIONS).expect("ITERATIONS fits in u32");
    eprintln!(
        "{label:<30}: {elapsed:?} total, {per_op:?} per request ({ITERATIONS} ops)",
        per_op = elapsed / iterations_u32,
    );
    elapsed
}

#[tokio::test]
#[ignore = "microbenchmark; run with --ignored --nocapture"]
async fn bench_no_layer_baseline() {
    let elapsed = time_loop("baseline (no layer)", ok_service()).await;
    // Sanity: 50k no-op tower calls should fit in well under a second on
    // any reasonable machine. Used as the dividend for overhead %.
    assert!(elapsed.as_secs() < 5, "baseline regressed unexpectedly: {elapsed:?}");
}

#[tokio::test]
#[ignore = "microbenchmark; run with --ignored --nocapture"]
async fn bench_http_metrics_layer_overhead() {
    // Force the recorder up first; otherwise the first metric call pays
    // the install cost which is not representative of steady-state.
    let _ = shared_handle();
    let svc = HttpMetricsLayer.layer(ok_service());
    time_loop("HttpMetricsLayer", svc).await;
}
