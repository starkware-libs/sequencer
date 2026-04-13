//! OHTTP test helpers.
//!
//! Two layers live in this module:
//!
//! - **Public primitives** (`pub fn`) — usable by any downstream crate that enables the `testing`
//!   feature. These cover the client-side BHTTP/HPKE round trip and a deterministic test gateway.
//!   They deliberately don't touch tower `Service` types or the inner service's body
//!   representation.
//!
//! - **Internal fixtures** (`pub(crate) ...`) — `TestHarness`, `echo_service`, `not_found_service`,
//!   `collect_body`. Coupled to `Full<Bytes>` as the inner service body type because that's what
//!   `tower_ohttp`'s own tests exercise. Consumers of the crate write their own echo services (with
//!   their framework body type) and build requests with the public primitives.

use std::io::Cursor;
use std::sync::Arc;

#[cfg(test)]
use bytes::Bytes;
#[cfg(test)]
use http::header;
#[cfg(test)]
use http_body_util::{BodyExt, Full};
#[cfg(test)]
use tower::{BoxError, Service};

use crate::OhttpGateway;

// ---------- public primitives (testing feature) ----------

/// Deterministic test gateway with a fixed key (first byte = 1, rest = 0).
/// Re-used across tests so they don't each have to repeat the key setup.
pub fn test_gateway() -> Arc<OhttpGateway> {
    let mut ikm = [0u8; 32];
    ikm[0] = 1;
    Arc::new(OhttpGateway::from_ikm(0, ohttp::hpke::Kem::X25519Sha256, &ikm).unwrap())
}

/// Client-side: build a BHTTP request for `(method, path, body, extra_headers)`
/// and encrypt it with the gateway's published key config. Returns the outer
/// encrypted envelope bytes plus the `ClientResponse` state that must be
/// passed to `decapsulate_bhttp_response` to decrypt the matching response.
pub fn encapsulate_bhttp_request(
    gateway: &OhttpGateway,
    method: &str,
    path: &str,
    body: &[u8],
    extra_headers: &[(&str, &[u8])],
) -> (Vec<u8>, ohttp::ClientResponse) {
    let mut bhttp_request = bhttp::Message::request(
        method.as_bytes().to_vec(),
        b"https".to_vec(),
        b"".to_vec(),
        path.as_bytes().to_vec(),
    );
    for (name, value) in extra_headers {
        bhttp_request.put_header(*name, *value);
    }
    bhttp_request.write_content(body);

    let mut bhttp_bytes = Vec::new();
    bhttp_request.write_bhttp(bhttp::Mode::KnownLength, &mut bhttp_bytes).unwrap();

    let client_request =
        ohttp::ClientRequest::from_encoded_config_list(gateway.encoded_config()).unwrap();
    client_request.encapsulate(&bhttp_bytes).unwrap()
}

/// Decapsulated OHTTP response: inner status code, inner body, and the full
/// parsed BHTTP message (for assertions on preserved headers like
/// `content-encoding`).
pub struct DecapsulatedOhttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
    pub bhttp_message: bhttp::Message,
}

/// Client-side: decrypt an OHTTP response envelope using the matching
/// `ClientResponse` context, parse the inner BHTTP message, and return the
/// status + body + the full BHTTP message.
pub fn decapsulate_bhttp_response(
    client_response: ohttp::ClientResponse,
    encrypted_response: &[u8],
) -> DecapsulatedOhttpResponse {
    let bhttp_bytes = client_response.decapsulate(encrypted_response).unwrap();
    let bhttp_message = bhttp::Message::read_bhttp(&mut Cursor::new(&bhttp_bytes)).unwrap();
    let status = bhttp_message.control().status().map(|s| s.code()).unwrap_or(0);
    let body = bhttp_message.content().to_vec();
    DecapsulatedOhttpResponse { status, body, bhttp_message }
}

// ---------- internal fixtures (tower_ohttp's own tests only) ----------

#[cfg(test)]
pub(crate) use internal::*;

#[cfg(test)]
mod internal {
    use super::*;

    /// Echo service: returns method, path, and content-type in response
    /// headers (prefixed `x-echo-*`), with the request body echoed as the
    /// response body.
    pub(crate) async fn echo_service(
        request: http::Request<Full<Bytes>>,
    ) -> Result<http::Response<Full<Bytes>>, BoxError> {
        let method = request.method().to_string();
        let path = request.uri().path().to_string();
        let content_type = request
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let body_bytes =
            request.into_body().collect().await.map(|c| c.to_bytes()).unwrap_or_default();

        Ok(http::Response::builder()
            .status(http::StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-echo-method", method)
            .header("x-echo-path", path)
            .header("x-echo-content-type", content_type)
            .body(Full::new(body_bytes))
            .unwrap())
    }

    /// Service that always returns a 404 response with a JSON body.
    pub(crate) async fn not_found_service(
        _request: http::Request<Full<Bytes>>,
    ) -> Result<http::Response<Full<Bytes>>, BoxError> {
        Ok(http::Response::builder()
            .status(http::StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Full::new(Bytes::from_static(br#"{"error":"not found"}"#)))
            .unwrap())
    }

    /// Test harness that runs OHTTP round trips against an `OhttpLayer`-wrapped
    /// service. Uses the public primitives above for the client-side BHTTP/HPKE
    /// work and wraps them in a convenience `ohttp_round_trip` method that also
    /// handles outer-request construction and envelope assertions.
    pub(crate) struct TestHarness<S> {
        pub gateway: Arc<OhttpGateway>,
        pub svc: S,
    }

    impl<S, ResBody> TestHarness<S>
    where
        ResBody: http_body::Body<Data = Bytes> + Send + 'static,
        ResBody::Error: Into<BoxError>,
        S: Service<
                http::Request<Full<Bytes>>,
                Response = http::Response<ResBody>,
                Error = BoxError,
            > + Send,
    {
        /// Encapsulate a BHTTP request and run it through the service. Asserts
        /// the outer envelope is 200 with `message/ohttp-res` content type.
        pub async fn ohttp_round_trip(
            &mut self,
            method: &str,
            path: &str,
            body: &[u8],
            extra_headers: &[(&str, &[u8])],
        ) -> DecapsulatedOhttpResponse {
            let (encapsulated, client_response) =
                encapsulate_bhttp_request(&self.gateway, method, path, body, extra_headers);

            let request = http::Request::builder()
                .method("POST")
                .uri("/")
                .header(header::CONTENT_TYPE, "message/ohttp-req")
                .body(Full::new(Bytes::from(encapsulated)))
                .unwrap();

            let response = self.svc.call(request).await.unwrap();
            assert_eq!(response.status(), http::StatusCode::OK);
            assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "message/ohttp-res");

            let encrypted_body = response
                .into_body()
                .collect()
                .await
                .map_err(|_| "failed to read layer response body")
                .unwrap()
                .to_bytes();
            decapsulate_bhttp_response(client_response, &encrypted_body)
        }

        /// Send raw bytes as an OHTTP request (for error-path tests).
        pub async fn send_raw_ohttp(&mut self, raw_body: Vec<u8>) -> http::Response<ResBody> {
            let request = http::Request::builder()
                .method("POST")
                .uri("/")
                .header(header::CONTENT_TYPE, "message/ohttp-req")
                .body(Full::new(Bytes::from(raw_body)))
                .unwrap();
            self.svc.call(request).await.unwrap()
        }

        /// Send a plaintext (non-OHTTP) request.
        pub async fn send_plaintext(&mut self, body: &[u8]) -> http::Response<ResBody> {
            let request = http::Request::builder()
                .method("POST")
                .uri("/")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Full::new(Bytes::copy_from_slice(body)))
                .unwrap();
            self.svc.call(request).await.unwrap()
        }
    }

    /// Collect a layer response body into bytes for assertions.
    pub(crate) async fn collect_body<ResBody>(response: http::Response<ResBody>) -> Bytes
    where
        ResBody: http_body::Body<Data = Bytes> + Send + 'static,
        ResBody::Error: Into<BoxError>,
    {
        response.into_body().collect().await.map_err(|_| "body err").unwrap().to_bytes()
    }
}
