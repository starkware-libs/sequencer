//! Integration tests for the sequencer's OHTTP wiring. These tests exercise
//! the `tower_ohttp::OhttpLayer` with `jsonrpsee::server::HttpBody` as the
//! response body type and the same middleware stack used in production
//! (`OhttpLayer` outermost, `CompressionLayer` between OHTTP and the inner
//! service).
//!
//! The body-type-agnostic layer behavior (method/path/status/content-type
//! preservation, error paths, body size limits, passthrough) is covered by
//! unit tests inside `tower_ohttp` itself. This file adds the jsonrpsee-
//! specific end-to-end coverage that can't live in the shared crate.
//!
//! Client-side BHTTP/HPKE primitives come from `tower_ohttp::test_utils`
//! (enabled via the crate's `testing` feature in this crate's dev-deps).

use std::io::Read;

use flate2::read::GzDecoder;
use http::header;
use http_body_util::{BodyExt, Full};
use jsonrpsee::server::HttpBody;
use tower::{BoxError, Layer, Service};
use tower_http::compression::CompressionLayer;
use tower_http::map_response_body::MapResponseBodyLayer;
use tower_ohttp::test_utils::{
    decapsulate_bhttp_response,
    encapsulate_bhttp_request,
    test_gateway,
};
use tower_ohttp::OhttpLayer;

const DEFAULT_BODY_LIMIT: usize = 102_400;
const KEY_CACHE_SECS: u64 = 3600;

/// Body builder for jsonrpsee's `HttpBody`. Returned as a `fn` pointer to
/// give `OhttpLayer` a sized, `Copy` closure type without an `as` cast.
fn body_builder() -> fn(Full<bytes::Bytes>) -> HttpBody {
    HttpBody::new
}

/// Echo service with jsonrpsee's `HttpBody` on both sides — matches the
/// layer's new symmetric-body inner service bound.
async fn jsonrpsee_echo_service(
    request: http::Request<HttpBody>,
) -> Result<http::Response<HttpBody>, BoxError> {
    let body_bytes = request.into_body().collect().await.map(|c| c.to_bytes()).unwrap_or_default();
    Ok(http::Response::builder()
        .status(http::StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(HttpBody::from(body_bytes.to_vec()))
        .unwrap())
}

/// Build the outer `http::Request<HttpBody>` that the layer receives after
/// the client has encapsulated a BHTTP message.
fn ohttp_http_request(encapsulated: Vec<u8>) -> http::Request<HttpBody> {
    http::Request::builder()
        .method("POST")
        .uri("/")
        .header(header::CONTENT_TYPE, "message/ohttp-req")
        .body(HttpBody::from(encapsulated))
        .unwrap()
}

/// Verify the OHTTP layer round-trips correctly with the jsonrpsee body type.
#[tokio::test]
async fn ohttp_round_trip_with_jsonrpsee_body() {
    let gateway = test_gateway();
    let layer =
        OhttpLayer::new(gateway.clone(), DEFAULT_BODY_LIMIT, KEY_CACHE_SECS, body_builder());
    let mut svc = layer.layer(tower::service_fn(jsonrpsee_echo_service));

    let json_body = br#"{"jsonrpc":"2.0","method":"starknet_specVersion","id":1}"#;
    let (encapsulated, client_response) =
        encapsulate_bhttp_request(&gateway, "POST", "/", json_body, &[]);

    let response = svc.call(ohttp_http_request(encapsulated)).await.unwrap();
    assert_eq!(response.status(), http::StatusCode::OK);
    assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "message/ohttp-res");

    let encrypted_body = response.into_body().collect().await.unwrap().to_bytes();
    let decapsulated = decapsulate_bhttp_response(client_response, &encrypted_body);
    assert_eq!(decapsulated.status, 200);
    assert_eq!(decapsulated.body, json_body);
}

/// Verify compress-then-encrypt: CompressionLayer sits between OhttpLayer and
/// the inner service (matching the production middleware stack). The client's
/// `Accept-Encoding: gzip` header flows through BHTTP to the compression
/// middleware, which compresses the echoed body before OHTTP encryption.
#[tokio::test]
async fn compress_then_encrypt_round_trip() {
    let gateway = test_gateway();
    let ohttp_layer =
        OhttpLayer::new(gateway.clone(), DEFAULT_BODY_LIMIT, KEY_CACHE_SECS, body_builder());
    // CompressionLayer wraps the response body as `CompressionBody<HttpBody>`.
    // Map it back to `HttpBody` so OhttpLayer's inner service bound
    // (`Response = Response<HttpBody>`) is satisfied.
    let compressed_echo = tower::ServiceBuilder::new()
        .layer(MapResponseBodyLayer::new(HttpBody::new))
        .layer(CompressionLayer::new())
        .service(tower::service_fn(jsonrpsee_echo_service));
    let mut svc = ohttp_layer.layer(compressed_echo);

    // Body must be large enough for gzip to actually compress.
    let body_json = serde_json::json!({
        "jsonrpc": "2.0",
        "result": { "data": "x".repeat(512) },
        "id": 1
    })
    .to_string();
    let body_bytes = body_json.as_bytes();

    let (encapsulated, client_response) = encapsulate_bhttp_request(
        &gateway,
        "POST",
        "/",
        body_bytes,
        &[("accept-encoding", b"gzip")],
    );

    let response = svc.call(ohttp_http_request(encapsulated)).await.unwrap();
    assert_eq!(response.status(), http::StatusCode::OK);

    let encrypted_body = response.into_body().collect().await.unwrap().to_bytes();
    let decapsulated = decapsulate_bhttp_response(client_response, &encrypted_body);
    assert_eq!(decapsulated.status, 200);

    let content_encoding = decapsulated.bhttp_message.header().get(b"content-encoding");
    assert!(content_encoding.is_some(), "expected content-encoding header (compress-then-encrypt)");
    assert_eq!(content_encoding.unwrap(), b"gzip");
    assert!(
        decapsulated.body.len() < body_bytes.len(),
        "compressed ({} B) should be smaller than original ({} B)",
        decapsulated.body.len(),
        body_bytes.len()
    );

    let mut decoder = GzDecoder::new(decapsulated.body.as_slice());
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed).expect("gzip decompression failed");
    assert_eq!(decompressed, body_bytes);
}

/// Verify that a plaintext (non-OHTTP) request passes through the layer
/// to the inner jsonrpsee service unchanged.
#[tokio::test]
async fn non_ohttp_request_passes_through_jsonrpsee() {
    let gateway = test_gateway();
    let layer =
        OhttpLayer::new(gateway.clone(), DEFAULT_BODY_LIMIT, KEY_CACHE_SECS, body_builder());
    let mut svc = layer.layer(tower::service_fn(jsonrpsee_echo_service));

    let json_body = br#"{"jsonrpc":"2.0","method":"starknet_specVersion","id":1}"#;
    let request = http::Request::builder()
        .method("POST")
        .uri("/")
        .header(header::CONTENT_TYPE, "application/json")
        .body(HttpBody::from(json_body.to_vec()))
        .unwrap();

    let response = svc.call(request).await.unwrap();
    assert_eq!(response.status(), http::StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body.as_ref(), json_body);
}
