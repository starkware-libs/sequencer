//! HTTP `/health` endpoint as a tower middleware layer.
//!
//! Short-circuits `GET /health` before the jsonrpsee service sees the request
//! (which would 405 a GET). Any other request passes through unchanged.

use std::task::{Context, Poll};

use bytes::Bytes;
use futures::future::{ready, Either, Ready};
use http::{header, Method, Request, Response, StatusCode};
use http_body_util::Full;
use jsonrpsee::server::HttpBody;
use tower::{Layer, Service};

#[cfg(test)]
#[path = "health_test.rs"]
mod health_test;

pub const HEALTH_PATH: &str = "/health";

const HEALTHY_BODY: &[u8] = br#"{"status":"ok"}"#;

#[derive(Clone, Copy, Default)]
pub struct HealthLayer;

impl<S> Layer<S> for HealthLayer {
    type Service = HealthService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        HealthService { inner }
    }
}

#[derive(Clone)]
pub struct HealthService<S> {
    inner: S,
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
            let response = Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/json")
                .body(HttpBody::new(Full::new(Bytes::from_static(HEALTHY_BODY))))
                .expect("response build with a static body is infallible");
            return Either::Left(ready(Ok(response)));
        }
        Either::Right(self.inner.call(request))
    }
}
