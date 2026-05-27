//! tower middleware that logs one structured line per HTTP request and
//! propagates a request id.
//!
//! This is the outermost, envelope-level layer: it emits one log line with
//! `event="http_request"`, `request_id`, `method`, `path`, `status`, and
//! `latency_ms` per HTTP request, and echoes `request_id` on the response so
//! callers can quote it. The id is accepted from the incoming `x-request-id`
//! header or generated as a UUID v4.
//!
//! It deliberately does NOT bind the id to a span covering the downstream
//! dispatch. For OHTTP traffic this layer runs on the *outer* envelope, whose
//! id is visible to the relay (echoed on the ciphertext response). Propagating
//! that id into the logs describing the *decapsulated* contents would create a
//! join key linking the relay's view (who) to the gateway's view (what),
//! defeating OHTTP unlinkability. Content-level correlation — a fresh,
//! envelope-unlinkable id bound below the OHTTP layer — is added by a
//! follow-up layer.
//!
//! Body bytes are never inspected — transaction calldata is private user data
//! per the privacy-pool threat model.

use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use http::{HeaderValue, Request, Response};
use jsonrpsee::server::HttpBody;
use tower::{Layer, Service};
use tracing::info;

#[cfg(test)]
#[path = "request_log_test.rs"]
mod request_log_test;

/// HTTP header carrying the request id.
pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// Cap on accepted incoming request-id length. Anything longer is dropped
/// in favour of a freshly generated id so the value never balloons into
/// AsyncLocalStorage / tracing fields and so log aggregators don't have
/// to parse megabyte-scale ids.
const MAX_REQUEST_ID_LEN: usize = 128;

/// Cap on the logged request path. The URI is attacker-controlled and this
/// layer is outermost, so an over-long path would bloat every log line; it is
/// truncated for logging only (the request itself is untouched).
const MAX_LOG_PATH_LEN: usize = 256;

pub(crate) fn new_request_id() -> String {
    uuid::Uuid::new_v4().to_string()
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

impl<S, ReqB> Service<Request<ReqB>> for RequestLogService<S>
where
    S: Service<Request<ReqB>, Response = Response<HttpBody>>,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
{
    type Response = Response<HttpBody>;
    type Error = S::Error;
    type Future =
        Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut request: Request<ReqB>) -> Self::Future {
        let request_id = extract_or_generate_request_id(&request);
        if let Ok(header_value) = HeaderValue::from_str(&request_id) {
            request.headers_mut().insert(REQUEST_ID_HEADER, header_value);
        }
        let method = request.method().clone();
        let path = truncated_log_path(request.uri().path());
        let start = Instant::now();

        let future = self.inner.call(request);

        Box::pin(async move {
            let result = future.await;
            let latency_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
            match result {
                Ok(mut response) => {
                    let status = response.status().as_u16();
                    if let Ok(header_value) = HeaderValue::from_str(&request_id) {
                        response.headers_mut().insert(REQUEST_ID_HEADER, header_value);
                    }
                    info!(
                        event = "http_request",
                        request_id = %request_id,
                        method = %method,
                        path = %path,
                        status = status,
                        latency_ms = latency_ms,
                        "HTTP request handled."
                    );
                    Ok(response)
                }
                Err(err) => {
                    info!(
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
