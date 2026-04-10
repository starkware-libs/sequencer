//! Tower [`Layer`] that adds OHTTP (RFC 9458) support to the JSON-RPC server.
//!
//! When applied as the outermost HTTP middleware:
//! - `GET /ohttp-keys` returns the HPKE key configuration (bypasses inner service)
//! - Requests with `Content-Type: message/ohttp-req` are decapsulated, forwarded as plain JSON-RPC,
//!   and the response is encapsulated
//! - All other requests pass through unchanged

use std::io::Cursor;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use http::header;
use http_body_util::{BodyExt, LengthLimitError, Limited};
use jsonrpsee::server::{HttpBody, HttpResponse};
use tower::{BoxError, Layer, Service, ServiceExt};
use tracing::debug;

use super::errors::OhttpProcessingError;
use super::gateway::OhttpGateway;
use super::{OHTTP_KEYS_PATH, OHTTP_REQUEST_CONTENT_TYPE, OHTTP_RESPONSE_CONTENT_TYPE};

/// Shared runtime state for the OHTTP gateway.
struct OhttpState {
    gateway: Arc<OhttpGateway>,
    body_limit: usize,
    key_cache_max_age_secs: u64,
}

/// Tower [`Layer`] that adds OHTTP envelope encryption.
#[derive(Clone)]
pub struct OhttpLayer(Arc<OhttpState>);

impl OhttpLayer {
    pub fn new(gateway: Arc<OhttpGateway>, body_limit: usize, key_cache_max_age_secs: u64) -> Self {
        Self(Arc::new(OhttpState { gateway, body_limit, key_cache_max_age_secs }))
    }
}

impl<S> Layer<S> for OhttpLayer {
    type Service = OhttpService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        OhttpService { inner, state: self.0.clone() }
    }
}

/// Tower [`Service`] that handles OHTTP encapsulation/decapsulation.
///
/// Generic over `ReqBody` so it integrates with both jsonrpsee's non-TLS path
/// (`ReqBody = HttpBody`) and TLS path (`ReqBody = hyper::body::Incoming`).
/// All incoming bodies are converted to [`HttpBody`] before further processing.
#[derive(Clone)]
pub struct OhttpService<S> {
    inner: S,
    state: Arc<OhttpState>,
}

impl<S, ReqBody, ResBody> Service<http::Request<ReqBody>> for OhttpService<S>
where
    ReqBody: http_body::Body<Data = Bytes> + Send + 'static,
    ReqBody::Error: Into<BoxError>,
    S: Service<http::Request<HttpBody>, Response = HttpResponse<ResBody>, Error = BoxError>
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
    ResBody: http_body::Body<Data = Bytes> + Send + 'static,
    ResBody::Error: Into<BoxError>,
{
    type Response = HttpResponse<HttpBody>;
    type Error = BoxError;
    type Future =
        Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: http::Request<ReqBody>) -> Self::Future {
        if request.method() == http::Method::GET && request.uri().path() == OHTTP_KEYS_PATH {
            let cache_control = format!("public, max-age={}", self.state.key_cache_max_age_secs);
            let key_config = self.state.gateway.encoded_config().to_vec();
            return Box::pin(async move {
                Ok(http::Response::builder()
                    .status(http::StatusCode::OK)
                    .header(header::CONTENT_TYPE, "application/ohttp-keys")
                    .header(header::CACHE_CONTROL, cache_control)
                    .body(HttpBody::from(key_config))
                    .expect("response builder should not fail"))
            });
        }

        // Convert the generic body to HttpBody for uniform processing.
        let request = request.map(HttpBody::new);

        let is_ohttp = request
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|ct| ct.eq_ignore_ascii_case(OHTTP_REQUEST_CONTENT_TYPE));

        if is_ohttp {
            let state = self.state.clone();
            let inner = self.inner.clone();
            return Box::pin(handle_ohttp_request(request, inner, state));
        }

        // Passthrough — normalize the response body to HttpBody.
        let inner = self.inner.clone();
        Box::pin(async move {
            let response = inner.oneshot(request).await?;
            Ok(response.map(HttpBody::new))
        })
    }
}

async fn handle_ohttp_request<S, ResBody>(
    request: http::Request<HttpBody>,
    inner: S,
    state: Arc<OhttpState>,
) -> Result<HttpResponse<HttpBody>, BoxError>
where
    S: Service<http::Request<HttpBody>, Response = HttpResponse<ResBody>, Error = BoxError>
        + Send
        + 'static,
    S::Future: Send + 'static,
    ResBody: http_body::Body<Data = Bytes> + Send + 'static,
    ResBody::Error: Into<BoxError>,
{
    // Step 1 — Read the outer encrypted body with size limit.
    let encapsulated_bytes = match Limited::new(request.into_body(), state.body_limit)
        .collect()
        .await
        .map(|collected| collected.to_bytes())
    {
        Ok(bytes) => bytes,
        Err(error) => {
            return Ok(if error.downcast_ref::<LengthLimitError>().is_some() {
                OhttpProcessingError::BodyTooLarge.into()
            } else {
                OhttpProcessingError::BadRequestBody.into()
            });
        }
    };

    // Step 2 — HPKE decapsulation.
    let (bhttp_bytes, server_response) =
        match state.gateway.server().decapsulate(&encapsulated_bytes) {
            Ok(result) => result,
            Err(error) => {
                debug!("OHTTP decapsulation failed: {error}");
                return Ok(OhttpProcessingError::DecapsulationFailed.into());
            }
        };

    // Step 3 — Parse Binary HTTP (RFC 9292).
    let bhttp_message = match bhttp::Message::read_bhttp(&mut Cursor::new(&bhttp_bytes)) {
        Ok(message) => message,
        Err(error) => {
            debug!("Invalid Binary HTTP message: {error}");
            return Ok(OhttpProcessingError::InvalidFormat("Invalid Binary HTTP message").into());
        }
    };

    // Step 4 — Rebuild inner HTTP request.
    let inner_request = match rebuild_request(&bhttp_message) {
        Ok(request) => request,
        Err(response) => return Ok(response),
    };

    // Step 5 — Route through inner service (jsonrpsee + other middleware).
    let inner_response = inner.oneshot(inner_request).await?;

    // Step 6 — Encapsulate the response.
    Ok(encapsulate_response(inner_response, server_response).await)
}

/// Rebuild a standard HTTP request from a parsed Binary HTTP message.
#[allow(clippy::result_large_err)]
fn rebuild_request(
    bhttp_message: &bhttp::Message,
) -> Result<http::Request<HttpBody>, HttpResponse<HttpBody>> {
    let body = bhttp_message.content().to_vec();

    let mut builder = http::Request::builder()
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CONTENT_LENGTH, body.len());

    // Forward Accept-Encoding from the BHTTP request so the inner CompressionLayer
    // compresses the response before OHTTP encryption (compress-then-encrypt).
    if let Some(accept_encoding) = bhttp_message.header().get(b"accept-encoding") {
        builder = builder.header(header::ACCEPT_ENCODING, accept_encoding);
    }

    if let Some(path) = bhttp_message.control().path() {
        let path_str = String::from_utf8_lossy(path);
        builder = builder.uri(path_str.as_ref());
    }

    builder.body(HttpBody::from(body)).map_err(|error| {
        debug!("Failed to rebuild inner request: {error}");
        OhttpProcessingError::InvalidFormat("Failed to rebuild inner request").into()
    })
}

/// Encapsulate the handler's response as an OHTTP response.
async fn encapsulate_response<ResBody>(
    response: HttpResponse<ResBody>,
    server_response: ohttp::ServerResponse,
) -> HttpResponse<HttpBody>
where
    ResBody: http_body::Body<Data = Bytes> + Send + 'static,
    ResBody::Error: Into<BoxError>,
{
    let status = response.status();
    let response_headers = response.headers().clone();

    let response_body = match response.into_body().collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => {
            return OhttpProcessingError::InternalError("Failed to read response body").into();
        }
    };

    let bhttp_status = bhttp::StatusCode::try_from(u64::from(status.as_u16()))
        .unwrap_or(bhttp::StatusCode::try_from(500u64).unwrap());
    let mut bhttp_response = bhttp::Message::response(bhttp_status);
    for (name, value) in &response_headers {
        bhttp_response.put_header(name.as_str(), value.as_bytes());
    }
    bhttp_response.write_content(&response_body);

    let mut bhttp_bytes = Vec::new();
    if let Err(error) = bhttp_response.write_bhttp(bhttp::Mode::KnownLength, &mut bhttp_bytes) {
        debug!("Failed to encode Binary HTTP response: {error}");
        return OhttpProcessingError::InternalError("Failed to encode response").into();
    }

    match server_response.encapsulate(&bhttp_bytes) {
        Ok(encrypted) => http::Response::builder()
            .status(http::StatusCode::OK)
            .header(header::CONTENT_TYPE, OHTTP_RESPONSE_CONTENT_TYPE)
            .body(HttpBody::from(encrypted))
            .expect("response builder should not fail"),
        Err(error) => {
            debug!("Failed to encapsulate OHTTP response: {error}");
            OhttpProcessingError::InternalError("Failed to encrypt response").into()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::sync::Arc;

    use http::header;
    use http_body_util::BodyExt;
    use jsonrpsee::server::{HttpBody, HttpResponse};
    use tower::{BoxError, Layer, Service};
    use tower_http::compression::CompressionLayer;

    use crate::server::ohttp::gateway::OhttpGateway;
    use crate::server::ohttp::layer::OhttpLayer;

    const DEFAULT_BODY_LIMIT: usize = 102_400;
    const KEY_CACHE_SECS: u64 = 3600;

    /// Test harness that wires up OhttpLayer → echo service and provides
    /// helpers for OHTTP client-side encapsulation/decapsulation.
    struct TestHarness<S> {
        gateway: Arc<OhttpGateway>,
        svc: S,
    }

    /// Decapsulated OHTTP response.
    struct OhttpResponse {
        status: u16,
        body: Vec<u8>,
        bhttp_message: bhttp::Message,
    }

    fn echo_service(
        request: http::Request<HttpBody>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<HttpResponse<HttpBody>, BoxError>> + Send>,
    > {
        Box::pin(async move {
            let body_bytes =
                request.into_body().collect().await.map(|c| c.to_bytes()).unwrap_or_default();
            Ok(http::Response::builder()
                .status(http::StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/json")
                .body(HttpBody::from(body_bytes.to_vec()))
                .unwrap())
        })
    }

    impl
        TestHarness<
            super::OhttpService<
                tower::util::ServiceFn<
                    fn(
                        http::Request<HttpBody>,
                    ) -> std::pin::Pin<
                        Box<
                            dyn std::future::Future<
                                    Output = Result<HttpResponse<HttpBody>, BoxError>,
                                > + Send,
                        >,
                    >,
                >,
            >,
        >
    {
        fn new() -> Self {
            Self::with_body_limit(DEFAULT_BODY_LIMIT)
        }

        fn with_body_limit(body_limit: usize) -> Self {
            let gateway = {
                let mut ikm = [0u8; 32];
                ikm[0] = 1;
                Arc::new(OhttpGateway::from_hex_key(&hex::encode(ikm)).unwrap())
            };
            let layer = OhttpLayer::new(gateway.clone(), body_limit, KEY_CACHE_SECS);
            let echo_fn: fn(_) -> _ = echo_service;
            let svc = layer.layer(tower::service_fn(echo_fn));
            Self { gateway, svc }
        }
    }

    impl<S> TestHarness<S>
    where
        S: Service<http::Request<HttpBody>, Response = HttpResponse<HttpBody>, Error = BoxError>,
    {
        /// Send an OHTTP-encrypted request and decrypt the response.
        async fn ohttp_round_trip(
            &mut self,
            json_body: &[u8],
            extra_headers: &[(&str, &[u8])],
        ) -> OhttpResponse {
            let (encapsulated, client_response) = {
                let mut bhttp_request = bhttp::Message::request(
                    b"POST".to_vec(),
                    b"https".to_vec(),
                    b"".to_vec(),
                    b"/".to_vec(),
                );
                bhttp_request.put_header("content-type", b"application/json");
                for (name, value) in extra_headers {
                    bhttp_request.put_header(*name, *value);
                }
                bhttp_request.write_content(json_body);

                let mut bhttp_bytes = Vec::new();
                bhttp_request.write_bhttp(bhttp::Mode::KnownLength, &mut bhttp_bytes).unwrap();

                let client_request =
                    ohttp::ClientRequest::from_encoded_config_list(self.gateway.encoded_config())
                        .unwrap();
                client_request.encapsulate(&bhttp_bytes).unwrap()
            };

            let request = http::Request::builder()
                .method("POST")
                .uri("/")
                .header(header::CONTENT_TYPE, "message/ohttp-req")
                .body(HttpBody::from(encapsulated))
                .unwrap();

            let response = self.svc.call(request).await.unwrap();
            assert_eq!(response.status(), http::StatusCode::OK);
            assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "message/ohttp-res");

            let encrypted_body = response.into_body().collect().await.unwrap().to_bytes();
            let bhttp_bytes = client_response.decapsulate(&encrypted_body).unwrap();
            let bhttp_message = bhttp::Message::read_bhttp(&mut Cursor::new(&bhttp_bytes)).unwrap();
            let status = bhttp_message.control().status().map(|s| s.code()).unwrap_or(0);
            let body = bhttp_message.content().to_vec();
            OhttpResponse { status, body, bhttp_message }
        }

        /// Send raw bytes with OHTTP content type (for error-path tests).
        async fn send_raw_ohttp(&mut self, raw_body: Vec<u8>) -> HttpResponse<HttpBody> {
            let request = http::Request::builder()
                .method("POST")
                .uri("/")
                .header(header::CONTENT_TYPE, "message/ohttp-req")
                .body(HttpBody::from(raw_body))
                .unwrap();
            self.svc.call(request).await.unwrap()
        }

        /// Send a plaintext (non-OHTTP) request.
        async fn send_plaintext(&mut self, json_body: &[u8]) -> HttpResponse<HttpBody> {
            let request = http::Request::builder()
                .method("POST")
                .uri("/")
                .header(header::CONTENT_TYPE, "application/json")
                .body(HttpBody::from(json_body.to_vec()))
                .unwrap();
            self.svc.call(request).await.unwrap()
        }
    }

    #[tokio::test]
    async fn ohttp_request_decapsulates_and_encapsulates() {
        let mut harness = TestHarness::new();
        let json_body = br#"{"jsonrpc":"2.0","method":"starknet_specVersion","id":1}"#;

        let response = harness.ohttp_round_trip(json_body, &[]).await;

        assert_eq!(response.status, 200);
        assert_eq!(response.body, json_body);
    }

    #[tokio::test]
    async fn malformed_ohttp_returns_422() {
        let mut harness = TestHarness::new();

        let response = harness.send_raw_ohttp(b"not valid ohttp".to_vec()).await;
        assert_eq!(response.status(), http::StatusCode::UNPROCESSABLE_ENTITY);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "OHTTP_DECAPSULATION_FAILED");
    }

    #[tokio::test]
    async fn oversized_ohttp_body_returns_413() {
        let body_limit = 64;
        let mut harness = TestHarness::with_body_limit(body_limit);

        let response = harness.send_raw_ohttp(vec![0u8; body_limit + 1]).await;
        assert_eq!(response.status(), http::StatusCode::PAYLOAD_TOO_LARGE);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "OHTTP_BODY_TOO_LARGE");
    }

    #[tokio::test]
    async fn non_ohttp_request_passes_through() {
        let mut harness = TestHarness::new();
        let json_body = br#"{"jsonrpc":"2.0","method":"starknet_specVersion","id":1}"#;

        let response = harness.send_plaintext(json_body).await;
        assert_eq!(response.status(), http::StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body.as_ref(), json_body);
    }

    /// Verify the compress-then-encrypt pipeline: CompressionLayer sits between
    /// OhttpLayer and the inner service (matching the production middleware stack).
    #[tokio::test]
    async fn compress_then_encrypt_round_trip() {
        use std::io::Read;

        use flate2::read::GzDecoder;

        let gateway = {
            let mut ikm = [0u8; 32];
            ikm[0] = 1;
            Arc::new(OhttpGateway::from_hex_key(&hex::encode(ikm)).unwrap())
        };
        let ohttp_layer = OhttpLayer::new(gateway.clone(), DEFAULT_BODY_LIMIT, KEY_CACHE_SECS);
        let compressed_echo = CompressionLayer::new().layer(tower::service_fn(echo_service));
        let svc = ohttp_layer.layer(compressed_echo);
        let mut harness = TestHarness { gateway, svc };

        // Body must be large enough for gzip to actually compress.
        let json_body = serde_json::json!({
            "jsonrpc": "2.0",
            "result": { "data": "x".repeat(512) },
            "id": 1
        })
        .to_string();
        let json_bytes = json_body.as_bytes();

        let response = harness.ohttp_round_trip(json_bytes, &[("accept-encoding", b"gzip")]).await;
        assert_eq!(response.status, 200);

        let content_encoding = response.bhttp_message.header().get(b"content-encoding");
        assert!(
            content_encoding.is_some(),
            "expected content-encoding header (compress-then-encrypt)"
        );
        assert_eq!(content_encoding.unwrap(), b"gzip");
        assert!(
            response.body.len() < json_bytes.len(),
            "compressed ({} B) should be smaller than original ({} B)",
            response.body.len(),
            json_bytes.len()
        );

        let mut decoder = GzDecoder::new(response.body.as_slice());
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed).expect("gzip decompression failed");
        assert_eq!(decompressed, json_bytes);
    }
}
