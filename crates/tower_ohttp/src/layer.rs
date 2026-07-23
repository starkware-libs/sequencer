//! Tower `Layer` and `Service` that add OHTTP (RFC 9458) envelope encryption.
//!
//! Routing behavior:
//! - `GET /ohttp-keys` → HPKE key configuration (bypasses inner service)
//! - `Content-Type: message/ohttp-req` → decapsulate, forward, encapsulate
//! - Everything else → forwarded to the inner service unchanged, with the original streaming body.
//!   The layer does not buffer or inspect non-OHTTP request bodies, and `body_limit` does not apply
//!   to them.
//!
//! The layer wraps any `Service<Request<B>, Response = Response<B>>` where
//! `B` is the framework's native body type (same on both sides). For OHTTP
//! requests it buffers the encrypted envelope into `Full<Bytes>`, decapsulates
//! it, rebuilds the inner request, and converts its body to `B` via a
//! `body_builder` closure (`Fn(Full<Bytes>) -> B`). The same closure converts
//! the `Full<Bytes>` responses the layer constructs itself (OHTTP-encrypted,
//! error, `/ohttp-keys`) into `B`, and `Self::Error = S::Error` so the inner
//! service's error type flows through unchanged (e.g. `Infallible` for axum).
//!
//! Typical `body_builder` values: `HttpBody::new` for jsonrpsee,
//! `axum::body::Body::new` for axum, `std::convert::identity` when
//! `B = Full<Bytes>`.

use std::io::Cursor;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use http::header;
use http_body_util::{BodyExt, Full, LengthLimitError, Limited};
use tower::{BoxError, Layer, Service, ServiceExt};
use tracing::debug;

use crate::bhttp_codec::{encapsulate_response, rebuild_request};
use crate::errors::OhttpError;
use crate::gateway::OhttpGateway;
use crate::{OHTTP_KEYS_PATH, OHTTP_REQUEST_CONTENT_TYPE};

#[cfg(test)]
#[path = "layer_test.rs"]
mod layer_test;

/// Shared runtime state for the OHTTP gateway.
struct OhttpState<F> {
    gateway: Arc<OhttpGateway>,
    body_limit: usize,
    key_cache_max_age_secs: u64,
    /// Converts `Full<Bytes>` into the inner service's body type. Used for
    /// both directions: buffered request bodies on the way in, and the
    /// layer's owned responses (OHTTP-encrypted, error, `/ohttp-keys`) on
    /// the way out.
    body_builder: F,
}

/// Tower [`Layer`] that adds OHTTP envelope encryption.
pub struct OhttpLayer<F>(Arc<OhttpState<F>>);

impl<F> Clone for OhttpLayer<F> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<F> OhttpLayer<F> {
    /// Create a new OHTTP layer.
    ///
    /// # Parameters
    /// - `gateway`: HPKE key state (shared via `Arc` with any other copies).
    /// - `body_limit`: maximum size of an OHTTP request envelope, in bytes. Non-OHTTP requests
    ///   bypass the layer's body pipeline entirely and are not subject to this limit — the inner
    ///   service enforces its own.
    /// - `key_cache_max_age_secs`: `Cache-Control: public, max-age=…` value on the `/ohttp-keys`
    ///   response.
    /// - `body_builder`: function that converts a `Full<Bytes>` into the inner service's body type
    ///   `B`. Used when rebuilding the inner request from a decapsulated OHTTP envelope, and when
    ///   emitting responses the layer owns itself (OHTTP-encrypted, error, `/ohttp-keys`).
    ///   Non-OHTTP requests are forwarded to the inner service with their original body unchanged —
    ///   the closure is not invoked on their path. Examples:
    ///   - `std::convert::identity` when `B = Full<Bytes>`
    ///   - `jsonrpsee::server::HttpBody::new` (jsonrpsee consumers)
    ///   - `axum::body::Body::new` (axum consumers)
    pub fn new(
        gateway: Arc<OhttpGateway>,
        body_limit: usize,
        key_cache_max_age_secs: u64,
        body_builder: F,
    ) -> Self {
        Self(Arc::new(OhttpState { gateway, body_limit, key_cache_max_age_secs, body_builder }))
    }
}

impl<S, F> Layer<S> for OhttpLayer<F> {
    type Service = OhttpService<S, F>;

    fn layer(&self, inner: S) -> Self::Service {
        OhttpService { inner, state: self.0.clone() }
    }
}

/// Tower [`Service`] produced by [`OhttpLayer`].
pub struct OhttpService<S, F> {
    inner: S,
    state: Arc<OhttpState<F>>,
}

impl<S: Clone, F> Clone for OhttpService<S, F> {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone(), state: self.state.clone() }
    }
}

impl<S, F, B> Service<http::Request<B>> for OhttpService<S, F>
where
    B: http_body::Body<Data = Bytes> + Send + 'static,
    B::Error: Into<BoxError>,
    S: Service<http::Request<B>, Response = http::Response<B>> + Clone + Send + 'static,
    S::Error: std::fmt::Debug + Send + 'static,
    S::Future: Send + 'static,
    F: Fn(Full<Bytes>) -> B + Send + Sync + 'static,
{
    type Response = http::Response<B>;
    type Error = S::Error;
    type Future =
        Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: http::Request<B>) -> Self::Future {
        let inner = self.inner.clone();
        let state = self.state.clone();

        Box::pin(async move {
            let build_body = |full| (state.body_builder)(full);

            // Route 1: GET /ohttp-keys — return key config directly, no buffering.
            if request.method() == http::Method::GET && request.uri().path() == OHTTP_KEYS_PATH {
                let cache_control = format!("public, max-age={}", state.key_cache_max_age_secs);
                let key_config = state.gateway.encoded_config().to_vec();
                let response = http::Response::builder()
                    .status(http::StatusCode::OK)
                    .header(header::CONTENT_TYPE, "application/ohttp-keys")
                    .header(header::CACHE_CONTROL, cache_control)
                    .body(Full::new(Bytes::from(key_config)))
                    .expect("key config response builder should not fail");
                return Ok(response.map(build_body));
            }

            // Route 2: non-OHTTP → forward untouched. The layer does not buffer
            // the body, does not apply `body_limit`, and does not invoke
            // `body_builder`; the inner service consumes the original body directly.
            let is_ohttp = request
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|ct| ct.eq_ignore_ascii_case(OHTTP_REQUEST_CONTENT_TYPE));
            if !is_ohttp {
                return inner.oneshot(request).await;
            }

            // Route 3: OHTTP envelope → buffer into `Full<Bytes>` under `body_limit`,
            // decapsulate, parse BHTTP, rebuild inner request, forward, encapsulate.
            let body_bytes = match Limited::new(request.into_body(), state.body_limit)
                .collect()
                .await
                .map(|collected| collected.to_bytes())
            {
                Ok(bytes) => bytes,
                Err(error) => {
                    let err = if error.downcast_ref::<LengthLimitError>().is_some() {
                        OhttpError::BodyTooLarge
                    } else {
                        OhttpError::BadRequestBody
                    };
                    return Ok(err.into_response().map(build_body));
                }
            };

            let (bhttp_bytes, server_response) =
                match state.gateway.server().decapsulate(&body_bytes) {
                    Ok(result) => result,
                    Err(error) => {
                        debug!("OHTTP decapsulation failed: {error}");
                        return Ok(OhttpError::DecapsulationFailed.into_response().map(build_body));
                    }
                };

            // RFC 9458 §5.2: errors detected after successful OHTTP decapsulation
            // MUST be sent in an encapsulated response. Collapse all post-decap
            // fallible work into a single Result so `server_response` is used
            // exactly once — either for the encapsulated success response, or for
            // the encapsulated error response.
            let post_decap_result: Result<http::Response<B>, OhttpError> = async {
                let bhttp_message = bhttp::Message::read_bhttp(&mut Cursor::new(&bhttp_bytes))
                    .map_err(|error| {
                        debug!("Invalid Binary HTTP message: {error}");
                        OhttpError::InvalidFormat("Invalid Binary HTTP message")
                    })?;

                let inner_request = rebuild_request(&bhttp_message)?.map(build_body);

                inner.oneshot(inner_request).await.map_err(|error| {
                    debug!("Inner service error after successful OHTTP decapsulation: {error:?}");
                    OhttpError::Internal("inner service error after OHTTP decapsulation")
                })
            }
            .await;

            let response_to_encapsulate: http::Response<B> = match post_decap_result {
                Ok(response) => response,
                Err(err) => err.into_response().map(build_body),
            };

            match encapsulate_response(response_to_encapsulate, server_response).await {
                Ok(response) => Ok(response.map(build_body)),
                Err(err) => {
                    // `server_response` was consumed by the failing `encapsulate_response`
                    // call above — no second chance to encapsulate. Fall back to plaintext.
                    // This is the only branch that may legitimately leak a plaintext error
                    // after successful decapsulation; it indicates an internal bug (BHTTP
                    // encode / HPKE seal failure), not a routine error path.
                    Ok(err.into_response().map(build_body))
                }
            }
        })
    }
}
