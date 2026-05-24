//! tower middleware that logs one structured line per HTTP request and
//! propagates a request id.
//!
//! Sits ahead of jsonrpsee so every JSON-RPC POST (and any pass-through like
//! OHTTP key fetches) gets a single log line with `event="http_request"`,
//! `request_id`, `method`, `path`, `status`, and `latency_ms`. The id is
//! either accepted from the incoming `x-request-id` header or generated as a
//! 128-bit random hex string (no `uuid` dep — `rand::random` covers it).
//! The id is also returned on the response so callers can quote it when
//! reporting failures.
//!
//! Body bytes are never inspected — transaction calldata is private user data
//! per the privacy-pool threat model.

use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use http::{HeaderValue, Request, Response};
use jsonrpsee::server::HttpBody;
use rand::RngCore;
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

fn new_request_id() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    let mut hex = String::with_capacity(32);
    for byte in bytes {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// Accepts the incoming `x-request-id` only when it's a short printable
/// ASCII token. CR/LF would let a client smuggle headers into the
/// response; arbitrary bytes (including unicode) make the value unsafe
/// to round-trip through `HeaderValue::from_str`. Any reject falls back
/// to a freshly generated hex id.
fn extract_or_generate_request_id<B>(request: &Request<B>) -> String {
    request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty() && value.len() <= MAX_REQUEST_ID_LEN)
        .filter(|value| value.bytes().all(is_safe_request_id_byte))
        .map(|value| value.to_string())
        .unwrap_or_else(new_request_id)
}

fn is_safe_request_id_byte(b: u8) -> bool {
    // Printable ASCII excluding whitespace. Rejects CR/LF/NUL/DEL etc.
    b.is_ascii_graphic()
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
        // Ensure the header reflects the (possibly generated) id so inner
        // services that re-read it see a consistent value.
        if let Ok(header_value) = HeaderValue::from_str(&request_id) {
            request.headers_mut().insert(REQUEST_ID_HEADER, header_value);
        }
        let method = request.method().clone();
        let path = request.uri().path().to_string();
        let start = Instant::now();

        let future = self.inner.call(request);
        let request_id_for_response = request_id.clone();

        Box::pin(async move {
            let result = future.await;
            // `as_millis` returns `u128`; saturate to `u64` so the tracing
            // field type stays simple. A request latency anywhere near 2^64
            // ms (584 million years) is unreachable.
            let latency_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
            match result {
                Ok(mut response) => {
                    let status = response.status().as_u16();
                    if let Ok(header_value) = HeaderValue::from_str(&request_id_for_response) {
                        response.headers_mut().insert(REQUEST_ID_HEADER, header_value);
                    }
                    info!(
                        event = "http_request",
                        request_id = %request_id_for_response,
                        method = %method,
                        path = %path,
                        status = status,
                        latency_ms = latency_ms,
                        "HTTP request handled."
                    );
                    Ok(response)
                }
                Err(err) => {
                    // The error path can't observe status, but still emit a
                    // log line so request timing is visible even on inner
                    // service failure.
                    info!(
                        event = "http_request",
                        request_id = %request_id_for_response,
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
