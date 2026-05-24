//! HTTP `/health` endpoint as a tower middleware layer.
//!
//! Short-circuits `GET /health` before the jsonrpsee service sees the request
//! (which would 405 a GET). With a `SaturationMonitor` attached, returns 503
//! once the service has been continuously rejecting requests for the
//! configured threshold so load balancers can drain the pod. Body is opaque
//! (no timestamps, counters, or upstream URLs).

use std::task::{Context, Poll};
use std::time::Duration;

use bytes::Bytes;
use futures::future::{ready, Either, Ready};
use http::{header, Method, Request, Response, StatusCode};
use http_body_util::Full;
use jsonrpsee::server::HttpBody;
use tower::{Layer, Service};

use crate::server::saturation::SaturationMonitor;

#[cfg(test)]
#[path = "health_test.rs"]
mod health_test;

pub const HEALTH_PATH: &str = "/health";

const HEALTHY_BODY: &[u8] = br#"{"status":"ok"}"#;
/// Body returned by `GET /health` when saturated. Reason is an opaque code,
/// no internal state included.
const SATURATED_BODY: &[u8] = br#"{"status":"unhealthy","reason":"saturated"}"#;

/// `saturation: None` keeps the original always-200 behaviour;
/// `Some(monitor)` flips to `503` once `monitor.saturated_for_at_least`
/// crosses `saturation_threshold`.
#[derive(Clone, Default)]
pub struct HealthLayer {
    saturation: Option<SaturationMonitor>,
    saturation_threshold: Duration,
}

impl HealthLayer {
    pub fn with_saturation(monitor: SaturationMonitor, threshold: Duration) -> Self {
        Self { saturation: Some(monitor), saturation_threshold: threshold }
    }
}

impl<S> Layer<S> for HealthLayer {
    type Service = HealthService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        HealthService {
            inner,
            saturation: self.saturation.clone(),
            saturation_threshold: self.saturation_threshold,
        }
    }
}

#[derive(Clone)]
pub struct HealthService<S> {
    inner: S,
    saturation: Option<SaturationMonitor>,
    saturation_threshold: Duration,
}

impl<S> HealthService<S> {
    fn health_response(&self) -> Response<HttpBody> {
        let saturated = self
            .saturation
            .as_ref()
            .is_some_and(|monitor| monitor.saturated_for_at_least(self.saturation_threshold));
        if saturated {
            Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .header(header::CONTENT_TYPE, "application/json")
                .body(HttpBody::new(Full::new(Bytes::from_static(SATURATED_BODY))))
                .expect("response build with a static body is infallible")
        } else {
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/json")
                .body(HttpBody::new(Full::new(Bytes::from_static(HEALTHY_BODY))))
                .expect("response build with a static body is infallible")
        }
    }
}

impl<S, ReqB> Service<Request<ReqB>> for HealthService<S>
where
    S: Service<Request<ReqB>, Response = Response<HttpBody>>,
{
    type Response = Response<HttpBody>;
    type Error = S::Error;
    type Future = Either<Ready<Result<Self::Response, Self::Error>>, S::Future>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // Always ready: the health fast-path doesn't need the inner service.
        // Inner backpressure is driven on demand by `inner.call` below.
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request<ReqB>) -> Self::Future {
        if request.method() == Method::GET && request.uri().path() == HEALTH_PATH {
            return Either::Left(ready(Ok(self.health_response())));
        }
        Either::Right(self.inner.call(request))
    }
}
