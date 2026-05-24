use bytes::Bytes;
use http::{Method, Request};
use http_body_util::Full;
use jsonrpsee::server::HttpBody;
use tower::{Layer, ServiceExt};
use tracing_test::traced_test;

use crate::server::health::HEALTH_PATH;
use crate::server::metrics::METRICS_PATH;
use crate::server::middleware_test_utils::{echo_request_id_service, read_body_and_headers};
use crate::server::request_log::{RequestLogLayer, REQUEST_ID_HEADER};

fn request_with_header(value: Option<&str>) -> Request<HttpBody> {
    let mut builder = Request::builder().method(Method::POST).uri("/");
    if let Some(value) = value {
        builder = builder.header(REQUEST_ID_HEADER, value);
    }
    builder.body(HttpBody::new(Full::new(Bytes::new()))).expect("static body is infallible")
}

#[tokio::test]
async fn echoes_supplied_request_id_on_response() {
    let svc = RequestLogLayer.layer(echo_request_id_service());

    let response = svc.oneshot(request_with_header(Some("client-supplied-id"))).await.unwrap();

    let (body, headers) = read_body_and_headers(response).await;
    assert_eq!(headers.get(REQUEST_ID_HEADER).unwrap(), "client-supplied-id");
    assert_eq!(body, "client-supplied-id");
}

#[tokio::test]
async fn generates_request_id_when_absent_and_echoes_it() {
    let svc = RequestLogLayer.layer(echo_request_id_service());

    let response = svc.oneshot(request_with_header(None)).await.unwrap();

    let (body, headers) = read_body_and_headers(response).await;
    let header_id = headers.get(REQUEST_ID_HEADER).unwrap().to_str().unwrap();
    // Body is what the inner service saw — they must match (the layer
    // rewrote the request header before forwarding).
    assert_eq!(header_id, body);
    // Canonical UUID v4: 8-4-4-4-12 hex with hyphens.
    assert!(uuid::Uuid::parse_str(header_id).is_ok(), "expected a UUID, got {header_id:?}");
    assert_eq!(uuid::Uuid::parse_str(header_id).unwrap().get_version_num(), 4);
}

#[tokio::test]
async fn drops_non_ascii_incoming_id_and_generates_a_fresh_one() {
    let mut request = request_with_header(None);
    request
        .headers_mut()
        .insert(REQUEST_ID_HEADER, http::HeaderValue::from_bytes(b"\xff\xfe").unwrap());

    let svc = RequestLogLayer.layer(echo_request_id_service());
    let response = svc.oneshot(request).await.unwrap();

    let (_body, headers) = read_body_and_headers(response).await;
    let header_id = headers.get(REQUEST_ID_HEADER).unwrap().to_str().unwrap();
    assert!(uuid::Uuid::parse_str(header_id).is_ok(), "should have generated a fresh UUID");
}

#[tokio::test]
async fn drops_request_id_containing_whitespace() {
    // CRLF in header values is rejected by the http crate itself at parse
    // time, so the residual concern is whitespace and other ASCII bytes
    // that would confuse log parsers if echoed verbatim into structured
    // fields.
    for hostile_id in ["with space", "tab\there", "leading space "] {
        let mut request = request_with_header(None);
        request.headers_mut().insert(
            REQUEST_ID_HEADER,
            http::HeaderValue::from_bytes(hostile_id.as_bytes()).unwrap(),
        );
        let svc = RequestLogLayer.layer(echo_request_id_service());
        let response = svc.oneshot(request).await.unwrap();
        let (_body, headers) = read_body_and_headers(response).await;
        let header_id = headers.get(REQUEST_ID_HEADER).unwrap().to_str().unwrap();
        assert!(
            uuid::Uuid::parse_str(header_id).is_ok(),
            "expected fresh UUID for hostile input {hostile_id:?}, got {header_id:?}",
        );
    }
}

#[tokio::test]
async fn drops_oversize_request_id() {
    let oversize_id = "a".repeat(2048);
    let mut request = request_with_header(None);
    request
        .headers_mut()
        .insert(REQUEST_ID_HEADER, http::HeaderValue::from_bytes(oversize_id.as_bytes()).unwrap());
    let svc = RequestLogLayer.layer(echo_request_id_service());
    let response = svc.oneshot(request).await.unwrap();
    let (_body, headers) = read_body_and_headers(response).await;
    let header_id = headers.get(REQUEST_ID_HEADER).unwrap().to_str().unwrap();
    assert!(uuid::Uuid::parse_str(header_id).is_ok());
}

#[tokio::test]
#[traced_test]
async fn health_probe_is_not_logged_but_still_gets_id_echo() {
    let svc = RequestLogLayer.layer(echo_request_id_service());
    let request = Request::builder()
        .method(Method::GET)
        .uri(HEALTH_PATH)
        .body(HttpBody::new(Full::new(Bytes::new())))
        .expect("static body is infallible");

    let response = svc.oneshot(request).await.unwrap();

    let (_body, headers) = read_body_and_headers(response).await;
    assert!(headers.contains_key(REQUEST_ID_HEADER), "id echo must still apply to probes");
    assert!(!logs_contain("http_request"), "health probes must not emit request log lines");
}

#[tokio::test]
#[traced_test]
async fn metrics_scrape_is_not_logged_but_still_gets_id_echo() {
    let svc = RequestLogLayer.layer(echo_request_id_service());
    let request = Request::builder()
        .method(Method::GET)
        .uri(METRICS_PATH)
        .body(HttpBody::new(Full::new(Bytes::new())))
        .expect("static body is infallible");

    let response = svc.oneshot(request).await.unwrap();

    let (_body, headers) = read_body_and_headers(response).await;
    assert!(headers.contains_key(REQUEST_ID_HEADER), "id echo must still apply to scrapes");
    assert!(!logs_contain("http_request"), "metrics scrapes must not emit request log lines");
}

#[tokio::test]
#[traced_test]
async fn non_health_request_is_logged() {
    let svc = RequestLogLayer.layer(echo_request_id_service());

    svc.oneshot(request_with_header(Some("logged-id"))).await.unwrap();

    assert!(logs_contain("http_request"));
    assert!(logs_contain("logged-id"));
}
