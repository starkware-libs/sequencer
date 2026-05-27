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
use tower_http::cors::CorsLayer;
use tower_http::map_request_body::MapRequestBodyLayer;
use tower_http::map_response_body::MapResponseBodyLayer;
use tower_ohttp::test_utils::{
    decapsulate_bhttp_response,
    encapsulate_bhttp_request,
    test_gateway,
};
use tower_ohttp::OhttpLayer;

use crate::server::request_log::{RequestLogLayer, REQUEST_ID_HEADER};
use crate::server::request_span::RequestSpanLayer;

const DEFAULT_BODY_LIMIT: usize = 102_400;
const KEY_CACHE_SECS: u64 = 3600;

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

/// Verify the full production `ServiceBuilder` chain compresses the *inner*
/// JSON-RPC response and leaves the *outer* OHTTP envelope uncompressed.
/// Mirrors the exact chain in `server.rs`/`tls.rs`, so any drift in layer
/// order or a missing `MapResponseBodyLayer` will break this test.
#[tokio::test]
async fn production_chain_compresses_inner_not_outer() {
    let gateway = test_gateway();
    let ohttp_layer =
        OhttpLayer::new(gateway.clone(), DEFAULT_BODY_LIMIT, KEY_CACHE_SECS, body_builder());

    // Replicates the OHTTP body-handling portion of the production chain in
    // `server.rs`/`tls.rs` (the outermost observability layers — request log,
    // health, metrics — don't affect body/compression handling and are
    // omitted). `RequestSpanLayer` is included since it sits inside OHTTP.
    let mut svc = tower::ServiceBuilder::new()
        .option_layer(None::<CorsLayer>)
        .layer(MapRequestBodyLayer::new(HttpBody::new))
        .option_layer(Some(ohttp_layer))
        .layer(RequestSpanLayer)
        .layer(MapResponseBodyLayer::new(HttpBody::new))
        .layer(CompressionLayer::new())
        .service(tower::service_fn(jsonrpsee_echo_service));

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
    assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "message/ohttp-res");
    assert!(
        response.headers().get(header::CONTENT_ENCODING).is_none(),
        "outer message/ohttp-res must not carry content-encoding",
    );

    let encrypted_body = response.into_body().collect().await.unwrap().to_bytes();
    let decapsulated = decapsulate_bhttp_response(client_response, &encrypted_body);
    assert_eq!(decapsulated.status, 200);

    let content_encoding = decapsulated.bhttp_message.header().get(b"content-encoding");
    assert!(
        content_encoding.is_some(),
        "expected content-encoding on inner body (compress-then-encrypt)"
    );
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

/// End-to-end OHTTP unlinkability: the request-id echoed on the OUTER
/// (relay-visible) response must differ from the fresh id bound to the
/// decapsulated inner dispatch, and the client-supplied inner id must be
/// discarded — so no shared key links the relay's view to the gateway's.
/// Exercises the real decapsulation path through
/// `RequestLogLayer → OhttpLayer → RequestSpanLayer`.
#[tokio::test]
async fn ohttp_inner_request_id_unlinkable_from_envelope() {
    let gateway = test_gateway();
    let ohttp_layer =
        OhttpLayer::new(gateway.clone(), DEFAULT_BODY_LIMIT, KEY_CACHE_SECS, body_builder());

    // Inner service echoes the request-id it observes into the response body.
    let echo_id = tower::service_fn(|req: http::Request<HttpBody>| async move {
        let id = req.headers().get(REQUEST_ID_HEADER).map(|v| v.to_str().unwrap()).unwrap_or("");
        Ok::<_, BoxError>(
            http::Response::builder()
                .status(http::StatusCode::OK)
                .body(HttpBody::from(id.as_bytes().to_vec()))
                .unwrap(),
        )
    });

    let mut svc = tower::ServiceBuilder::new()
        .layer(RequestLogLayer)
        .layer(MapRequestBodyLayer::new(HttpBody::new))
        .option_layer(Some(ohttp_layer))
        .layer(RequestSpanLayer)
        .layer(MapResponseBodyLayer::new(HttpBody::new))
        .service(echo_id);

    // The envelope carries a client-chosen inner id that must be discarded.
    let (encapsulated, client_response) = encapsulate_bhttp_request(
        &gateway,
        "POST",
        "/",
        b"",
        &[("x-request-id", b"inner-client-id")],
    );

    // The outer envelope request carries the relay-visible id.
    let mut outer = ohttp_http_request(encapsulated);
    outer
        .headers_mut()
        .insert(REQUEST_ID_HEADER, http::HeaderValue::from_static("envelope-relay-id"));

    let response = svc.call(outer).await.unwrap();

    // The outer (relay-visible) response echoes the envelope id.
    let envelope_id =
        response.headers().get(REQUEST_ID_HEADER).unwrap().to_str().unwrap().to_owned();
    assert_eq!(envelope_id, "envelope-relay-id");

    let encrypted_body = response.into_body().collect().await.unwrap().to_bytes();
    let decapsulated = decapsulate_bhttp_response(client_response, &encrypted_body);
    assert_eq!(decapsulated.status, 200);
    let inner_id = String::from_utf8(decapsulated.body).expect("utf8 inner id");

    assert_ne!(inner_id, envelope_id, "inner id must not equal the relay-visible envelope id");
    assert_ne!(inner_id, "inner-client-id", "client-supplied inner id must be discarded");
    assert!(
        uuid::Uuid::parse_str(&inner_id).is_ok(),
        "inner id must be a fresh UUID, got {inner_id:?}"
    );
    // No id is set on the inner *response*, so nothing — neither the envelope
    // id nor the fresh content id — leaks into the encrypted reply's headers.
    assert!(
        decapsulated.bhttp_message.header().get(b"x-request-id").is_none(),
        "inner OHTTP response must not carry an x-request-id header"
    );
}
