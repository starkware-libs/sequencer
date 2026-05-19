//! HTTP `/health` endpoint as a tower middleware layer.
//!
//! Short-circuits `GET /health` with a `200 OK` JSON response *before* the
//! jsonrpsee service sees the request, so health checks don't run through the
//! JSON-RPC parser (which would reject GETs with 405). Any other request is
//! passed through to the inner service unchanged.
//!
//! Returns the same `Response<HttpBody>` shape that the existing tower layers
//! (CORS, OHTTP, compression, body-mapping) already produce so the layer can be
//! placed outermost in the existing `set_http_middleware` chain.
//!
//! Body intentionally contains no service state: it must remain safe to expose
//! without authentication (no transaction data, no config, no version yet —
//! version is logged at startup instead, see `main.rs`).

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

/// Path served by [`HealthLayer`].
pub const HEALTH_PATH: &str = "/health";

/// Body returned by `GET /health`. Static and stateless — see module docs.
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

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
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
