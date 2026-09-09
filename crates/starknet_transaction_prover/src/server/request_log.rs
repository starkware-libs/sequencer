//! tower middleware that logs one structured line per HTTP request and
//! propagates a request id.
//!
//! This is the outermost, envelope-level layer: it emits one log line with
//! `event="http_request"`, `request_id`, `method`, `path`, `status`, and
//! `latency_ms` per HTTP request, and echoes `request_id` on the response so
//! callers can quote it. The id is accepted from the incoming `x-request-id`
//! header or generated as a UUID v4. `GET /health` probes and `GET /metrics`
//! scrapes still get the id echo but no log line. At typical probe and scrape
//! periods they would drown out real traffic.
//!
//! It deliberately does NOT bind the id to a span covering the downstream
//! dispatch. For OHTTP traffic this layer runs on the *outer* envelope, whose
//! id is visible to the relay (echoed on the ciphertext response). Propagating
//! that id into the logs describing the *decapsulated* contents would create a
//! join key linking the relay's view (who) to the gateway's view (what),
//! defeating OHTTP unlinkability. Content-level correlation requires a
//! separate, envelope-unlinkable id bound below the OHTTP layer.
//!
//! For OHTTP traffic `status` and `path` also describe the outer envelope: the
//! outer status is 200 whenever decapsulation succeeds (RFC 9458), so inner
//! JSON-RPC failures never appear in this line.
//!
//! Body bytes are never inspected — transaction calldata is private user data
//! per the privacy-pool threat model.

use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use http::{HeaderValue, Method, Request, Response};
use tower::{Layer, Service};
use tracing::{info, warn};

use crate::server::health::HEALTH_PATH;
use crate::server::metrics::METRICS_PATH;

#[cfg(test)]
#[path = "request_log_test.rs"]
mod request_log_test;

/// HTTP header carrying the request id.
pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// Request extension carrying the id this layer already validated/generated,
/// so a downstream layer (e.g. `RequestSpanLayer`) can reuse it on the
/// plaintext path instead of re-parsing and re-validating the header it just
/// set.
#[derive(Clone)]
pub(crate) struct RequestId(pub String);

/// tower [`Layer`] producing [`RequestLogService`].
#[derive(Clone, Copy, Default)]
pub struct RequestLogLayer;

impl<S> Layer<S> for RequestLogLayer {
    type Service = RequestLogService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequestLogService { inner }
    }
}

#[derive(Clone)]
pub struct RequestLogService<S> {
    inner: S,
}

impl<S, ReqB, RespB> Service<Request<ReqB>> for RequestLogService<S>
where
    S: Service<Request<ReqB>, Response = Response<RespB>>,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
    RespB: Send + 'static,
{
    type Response = Response<RespB>;
    type Error = S::Error;
    type Future =
        Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut request: Request<ReqB>) -> Self::Future {
        let request_id = extract_or_generate_request_id(&request);
        let id_header_value = request_id_header_value(&request_id);
        request.headers_mut().insert(REQUEST_ID_HEADER, id_header_value.clone());
        request.extensions_mut().insert(RequestId(request_id.clone()));
        let request_path = request.uri().path();
        let is_probe_or_scrape = request.method() == Method::GET
            && (request_path == HEALTH_PATH || request_path == METRICS_PATH);
        let method = request.method().clone();
        let path = truncated_log_path(request.uri().path());
        let start = Instant::now();

        let future = self.inner.call(request);

        Box::pin(async move {
            let result = future.await;
            let latency_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
            match result {
                Ok(mut response) => {
                    response.headers_mut().insert(REQUEST_ID_HEADER, id_header_value);
                    if !is_probe_or_scrape {
                        info!(
                            event = "http_request",
                            request_id = %request_id,
                            method = %method,
                            path = %path,
                            status = response.status().as_u16(),
                            latency_ms = latency_ms,
                            "HTTP request handled."
                        );
                    }
                    Ok(response)
                }
                Err(err) => {
                    // The only per-request observation point before hyper
                    // aborts the connection without a response.
                    warn!(
                        event = "http_request",
                        request_id = %request_id,
                        method = %method,
                        path = %path,
                        latency_ms = latency_ms,
                        outcome = "service_error",
                        "HTTP request failed in tower stack."
                    );
                    Err(err)
                }
            }
        })
    }
}

/// Cap on accepted incoming request-id length. Anything longer is dropped
/// in favour of a freshly generated id so the value never balloons into
/// tracing fields and log aggregators don't have to parse megabyte-scale ids.
const MAX_REQUEST_ID_LEN: usize = 128;

/// Cap on the logged request path. The URI is attacker-controlled and this
/// layer is outermost, so an over-long path would bloat every log line; it is
/// truncated for logging only (the request itself is untouched).
const MAX_LOG_PATH_LEN: usize = 256;

pub(crate) fn new_request_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Header value for an id produced by [`extract_or_generate_request_id`] or
/// [`new_request_id`] — always printable ASCII, so the conversion is
/// infallible.
pub(crate) fn request_id_header_value(request_id: &str) -> HeaderValue {
    HeaderValue::from_str(request_id)
        .expect("request id is printable ASCII by construction: fresh UUID or filtered header")
}

/// Accepts the incoming `x-request-id` only when it's a short printable
/// ASCII token. CR/LF would let a client smuggle headers into the
/// response; arbitrary bytes (including unicode) make the value unsafe
/// to round-trip through `HeaderValue::from_str`. Any reject falls back
/// to a freshly generated UUID v4.
pub(crate) fn extract_or_generate_request_id<B>(request: &Request<B>) -> String {
    request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty() && value.len() <= MAX_REQUEST_ID_LEN)
        .filter(|value| value.bytes().all(is_safe_request_id_byte))
        .map(|value| value.to_string())
        .unwrap_or_else(new_request_id)
}

fn is_safe_request_id_byte(byte: u8) -> bool {
    // Reject whitespace/CR/LF/NUL/control bytes so the id can't smuggle headers
    // into the response or break structured-log parsers.
    byte.is_ascii_graphic()
}

/// Truncates an over-long path on a char boundary for safe logging.
fn truncated_log_path(path: &str) -> String {
    if path.len() <= MAX_LOG_PATH_LEN {
        return path.to_string();
    }
    let mut end = MAX_LOG_PATH_LEN;
    while !path.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…(truncated)", &path[..end])
}
