//! tower middleware that binds an `http_request` tracing span over the
//! downstream dispatch. It sits BELOW the OHTTP layer, so it sees the
//! decapsulated inner request (or a plaintext pass-through), and picks the id:
//!
//! - **plaintext** — reuse the `x-request-id` the outer layer already assigned;
//! - **OHTTP-decapsulated** ([`tower_ohttp::Decapsulated`]) — mint a fresh id (any client-supplied
//!   inner id was already stripped at decapsulation). The relay never observes it, so the
//!   relay-visible envelope id and this content-log id share no join key. Note the residual: at low
//!   traffic volume, timestamp proximity in persisted logs still permits probabilistic correlation
//!   — an OHTTP traffic-analysis property id separation cannot eliminate.
//!
//! See [`super::request_log`] for why the envelope and content ids are kept
//! separate (OHTTP unlinkability).

use std::task::{Context, Poll};

use http::{Request, Response};
use tower::{Layer, Service};
use tower_ohttp::Decapsulated;
use tracing::instrument::Instrumented;
use tracing::{info_span, Instrument};

use crate::server::request_log::{
    extract_or_generate_request_id,
    new_request_id,
    request_id_header_value,
    RequestId,
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
            // Fresh id, distinct from the relay-visible envelope id (OHTTP
            // unlinkability). Inserted so downstream readers see the id the
            // span carries.
            let fresh_id = new_request_id();
            request.headers_mut().insert(REQUEST_ID_HEADER, request_id_header_value(&fresh_id));
            fresh_id
        } else {
            // Reuses the id `RequestLogLayer` already validated/generated via
            // its request extension, avoiding a second header parse and
            // validation pass per request. Removed rather than cloned: no
            // downstream reader needs the extension past this point, so
            // taking ownership skips a `String` allocation on every
            // plaintext request. Falls back to re-deriving it (the header is
            // left untouched either way) so the layer stays correct
            // standalone, e.g. in unit tests without `RequestLogLayer`
            // upstream.
            request
                .extensions_mut()
                .remove::<RequestId>()
                .map_or_else(|| extract_or_generate_request_id(&request), |request_id| request_id.0)
        };
        self.inner.call(request).instrument(info_span!("http_request", request_id = %request_id))
    }
}
