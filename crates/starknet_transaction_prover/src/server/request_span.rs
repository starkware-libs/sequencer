//! tower middleware that binds an `http_request` tracing span over the
//! downstream dispatch. It sits BELOW the OHTTP layer, so it sees the
//! decapsulated inner request (or a plaintext pass-through), and picks the id:
//!
//! - **plaintext** — reuse the `x-request-id` the outer layer already assigned;
//! - **OHTTP-decapsulated** ([`tower_ohttp::Decapsulated`]) — mint a fresh id, overwriting any
//!   client-supplied inner id. The relay never observes it (it lives in content logs and, at most,
//!   the encrypted inner response), so the relay-visible envelope id and this content-log id can't
//!   be joined.
//!
//! See [`super::request_log`] for why the envelope and content ids are kept
//! separate (OHTTP unlinkability).

use std::task::{Context, Poll};

use http::{HeaderValue, Request, Response};
use tower::{Layer, Service};
use tower_ohttp::Decapsulated;
use tracing::instrument::Instrumented;
use tracing::{info_span, Instrument};

use crate::server::request_log::{
    extract_or_generate_request_id,
    new_request_id,
    REQUEST_ID_HEADER,
};

#[cfg(test)]
#[path = "request_span_test.rs"]
mod request_span_test;

/// tower [`Layer`] producing [`RequestSpanService`].
#[derive(Clone, Copy, Default)]
pub struct RequestSpanLayer;

impl<S> Layer<S> for RequestSpanLayer {
    type Service = RequestSpanService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequestSpanService { inner }
    }
}

#[derive(Clone)]
pub struct RequestSpanService<S> {
    inner: S,
}

impl<S, ReqB, RespB> Service<Request<ReqB>> for RequestSpanService<S>
where
    S: Service<Request<ReqB>, Response = Response<RespB>>,
{
    type Response = Response<RespB>;
    type Error = S::Error;
    type Future = Instrumented<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut request: Request<ReqB>) -> Self::Future {
        let request_id = if request.extensions().get::<Decapsulated>().is_some() {
            // Fresh id, distinct from the relay-visible envelope id (OHTTP unlinkability).
            new_request_id()
        } else {
            // In production this re-derives the exact id `RequestLogLayer`
            // already assigned to the plaintext request, so logs and the echoed
            // response header share one id. Re-validating here (rather than
            // assuming the header) also keeps the layer correct when it runs
            // standalone, e.g. in unit tests without `RequestLogLayer` upstream.
            extract_or_generate_request_id(&request)
        };
        if let Ok(header_value) = HeaderValue::from_str(&request_id) {
            request.headers_mut().insert(REQUEST_ID_HEADER, header_value);
        }
        self.inner.call(request).instrument(info_span!("http_request", request_id = %request_id))
    }
}
