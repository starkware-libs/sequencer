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

#[cfg(test)]
mod tests {
    use tower::Layer;

    use crate::layer::OhttpLayer;
    use crate::test_utils::{
        collect_body,
        echo_service,
        failing_service,
        not_found_service,
        test_gateway,
        TestHarness,
    };

    const DEFAULT_BODY_LIMIT: usize = 102_400;
    const KEY_CACHE_SECS: u64 = 3600;

    /// A `fn` pointer alias for the identity body builder, so `OhttpLayer`'s
    /// `F` parameter has a stable sized type instead of `impl Fn`.
    type IdentityBody =
        fn(http_body_util::Full<bytes::Bytes>) -> http_body_util::Full<bytes::Bytes>;
    const IDENTITY_BODY_BUILDER: IdentityBody = std::convert::identity;

    fn test_layer_with_limit(body_limit: usize) -> OhttpLayer<IdentityBody> {
        OhttpLayer::new(test_gateway(), body_limit, KEY_CACHE_SECS, IDENTITY_BODY_BUILDER)
    }

    fn test_layer() -> OhttpLayer<IdentityBody> {
        test_layer_with_limit(DEFAULT_BODY_LIMIT)
    }

    #[tokio::test]
    async fn ohttp_request_round_trip() {
        let layer = test_layer();
        let svc = layer.layer(tower::service_fn(echo_service));
        let mut harness = TestHarness { gateway: test_gateway(), svc };

        let body = br#"{"jsonrpc":"2.0","method":"starknet_specVersion","id":1}"#;
        let response = harness.ohttp_round_trip("POST", "/", body, &[]).await;

        assert_eq!(response.status, 200);
        assert_eq!(response.body, body);
    }

    #[tokio::test]
    async fn non_post_method_round_trip() {
        // GET /health encapsulated in OHTTP must reach the inner service as GET.
        let layer = test_layer();
        let svc = layer.layer(tower::service_fn(echo_service));
        let mut harness = TestHarness { gateway: test_gateway(), svc };

        let response = harness.ohttp_round_trip("GET", "/health", b"", &[]).await;

        assert_eq!(response.status, 200);
        let method = response.bhttp_message.header().get(b"x-echo-method").unwrap();
        let path = response.bhttp_message.header().get(b"x-echo-path").unwrap();
        assert_eq!(method, b"GET");
        assert_eq!(path, b"/health");
    }

    #[tokio::test]
    async fn non_200_status_round_trip() {
        // A 404 from the inner service must be preserved inside the encrypted
        // BHTTP envelope; the outer OHTTP response is still 200.
        let layer = test_layer();
        let svc = layer.layer(tower::service_fn(not_found_service));
        let mut harness = TestHarness { gateway: test_gateway(), svc };

        let response = harness.ohttp_round_trip("GET", "/missing", b"", &[]).await;

        assert_eq!(response.status, 404);
        assert_eq!(response.body, br#"{"error":"not found"}"#);
    }

    #[tokio::test]
    async fn content_type_routing() {
        // content-type in the BHTTP request must be forwarded to the inner
        // service (not hardcoded to application/json).
        let layer = test_layer();
        let svc = layer.layer(tower::service_fn(echo_service));
        let mut harness = TestHarness { gateway: test_gateway(), svc };

        let response = harness
            .ohttp_round_trip("POST", "/", b"plain text", &[("content-type", b"text/plain")])
            .await;

        assert_eq!(response.status, 200);
        let echoed_content_type =
            response.bhttp_message.header().get(b"x-echo-content-type").unwrap();
        assert_eq!(echoed_content_type, b"text/plain");
    }

    #[tokio::test]
    async fn malformed_ohttp_returns_422() {
        let layer = test_layer();
        let svc = layer.layer(tower::service_fn(echo_service));
        let mut harness = TestHarness { gateway: test_gateway(), svc };

        let response = harness.send_raw_ohttp(b"not valid ohttp".to_vec()).await;
        assert_eq!(response.status(), http::StatusCode::UNPROCESSABLE_ENTITY);

        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "OHTTP_DECAPSULATION_FAILED");
    }

    #[tokio::test]
    async fn oversized_ohttp_body_returns_413() {
        let body_limit = 64;
        let layer = test_layer_with_limit(body_limit);
        let svc = layer.layer(tower::service_fn(echo_service));
        let mut harness = TestHarness { gateway: test_gateway(), svc };

        let response = harness.send_raw_ohttp(vec![0u8; body_limit + 1]).await;
        assert_eq!(response.status(), http::StatusCode::PAYLOAD_TOO_LARGE);

        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "OHTTP_BODY_TOO_LARGE");
    }

    #[tokio::test]
    async fn post_decap_bhttp_parse_error_is_encapsulated() {
        // HPKE-seal bytes that are NOT a valid BHTTP message. Decapsulation
        // succeeds, then BHTTP parsing fails. RFC 9458 §5.2: the resulting
        // error MUST be returned encapsulated, not as plaintext JSON.
        let layer = test_layer();
        let svc = layer.layer(tower::service_fn(echo_service));
        let mut harness = TestHarness { gateway: test_gateway(), svc };

        let response = harness.send_raw_inner_bhttp(b"NOT A VALID BHTTP MESSAGE").await;

        // `send_raw_inner_bhttp` already asserted the outer envelope is
        // `message/ohttp-res`. The inner (decapsulated) response is a 422 with
        // the JSON error body.
        assert_eq!(response.status, 422);
        let json: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(json["error"]["code"], "OHTTP_INVALID_FORMAT");
    }

    #[tokio::test]
    async fn post_decap_inner_service_error_is_encapsulated() {
        // Inner service returns Err after decapsulation succeeds. RFC 9458
        // §5.2: the error MUST be returned encapsulated as a 500.
        let layer = test_layer();
        let svc = layer.layer(tower::service_fn(failing_service));
        let mut harness = TestHarness { gateway: test_gateway(), svc };

        let response = harness.ohttp_round_trip("POST", "/", b"{}", &[]).await;

        assert_eq!(response.status, 500);
        let json: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(json["error"]["code"], "INTERNAL_ERROR");
    }

    #[tokio::test]
    async fn non_ohttp_request_passes_through() {
        let layer = test_layer();
        let svc = layer.layer(tower::service_fn(echo_service));
        let mut harness = TestHarness { gateway: test_gateway(), svc };

        let body = br#"{"jsonrpc":"2.0","method":"starknet_specVersion","id":1}"#;
        let response = harness.send_plaintext(body).await;
        assert_eq!(response.status(), http::StatusCode::OK);

        let echoed_body = collect_body(response).await;
        assert_eq!(echoed_body.as_ref(), body);
    }
}
