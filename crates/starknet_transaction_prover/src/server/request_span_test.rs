use bytes::Bytes;
use http::{Method, Request};
use http_body_util::Full;
use jsonrpsee::server::HttpBody;
use tower::{Layer, ServiceExt};
use tower_ohttp::Decapsulated;

use crate::server::middleware_test_utils::{echo_request_id_service, read_body_and_headers};
use crate::server::request_log::{RequestLogLayer, REQUEST_ID_HEADER};
use crate::server::request_span::RequestSpanLayer;

#[tokio::test]
async fn plaintext_reuses_inbound_request_id() {
    let request = Request::builder()
        .method(Method::POST)
        .uri("/")
        .header(REQUEST_ID_HEADER, "reused-xyz")
        .body(HttpBody::new(Full::new(Bytes::new())))
        .expect("static body is infallible");

    let response =
        RequestSpanLayer.layer(echo_request_id_service()).oneshot(request).await.unwrap();

    let (body, _headers) = read_body_and_headers(response).await;
    assert_eq!(body, "reused-xyz");
}

#[tokio::test]
async fn decapsulated_gets_fresh_id_discarding_inbound() {
    let mut request = Request::builder()
        .method(Method::POST)
        .uri("/")
        .header(REQUEST_ID_HEADER, "envelope-abc")
        .body(HttpBody::new(Full::new(Bytes::new())))
        .expect("static body is infallible");
    request.extensions_mut().insert(Decapsulated);

    let response =
        RequestSpanLayer.layer(echo_request_id_service()).oneshot(request).await.unwrap();

    let (id, _headers) = read_body_and_headers(response).await;
    assert_ne!(id, "envelope-abc", "must discard the client-supplied inner id");
    assert!(uuid::Uuid::parse_str(&id).is_ok(), "must mint a fresh UUID, got {id:?}");
}

/// The cross-layer plaintext contract: with `RequestLogLayer` (outer) stacked
/// over `RequestSpanLayer` (inner) and no inbound id, the id the outer layer
/// generates and echoes on the response must be the same id the inner layer
/// binds for the handler — one shared id end-to-end.
#[tokio::test]
async fn plaintext_log_and_span_layers_share_generated_id() {
    let request = Request::builder()
        .method(Method::POST)
        .uri("/")
        .body(HttpBody::new(Full::new(Bytes::new())))
        .expect("static body is infallible");

    let svc = RequestLogLayer.layer(RequestSpanLayer.layer(echo_request_id_service()));
    let response = svc.oneshot(request).await.unwrap();

    let (handler_id, headers) = read_body_and_headers(response).await;
    let echoed_id =
        headers.get(REQUEST_ID_HEADER).expect("response carries the id").to_str().unwrap();

    assert_eq!(echoed_id, handler_id, "echoed response id must equal the id the handler saw");
    assert!(
        uuid::Uuid::parse_str(&handler_id).is_ok(),
        "generated id must be a UUID, got {handler_id:?}"
    );
}
