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
type IdentityBody = fn(http_body_util::Full<bytes::Bytes>) -> http_body_util::Full<bytes::Bytes>;
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
    let echoed_content_type = response.bhttp_message.header().get(b"x-echo-content-type").unwrap();
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
