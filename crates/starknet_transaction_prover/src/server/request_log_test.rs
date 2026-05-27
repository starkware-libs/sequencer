use bytes::Bytes;
use http::{Method, Request, Response, StatusCode};
use http_body_util::{BodyExt, Full};
use jsonrpsee::server::HttpBody;
use tower::{Layer, ServiceExt};

use crate::server::request_log::{RequestLogLayer, REQUEST_ID_HEADER};

fn echo_request_id_service() -> impl tower::Service<
    Request<HttpBody>,
    Response = Response<HttpBody>,
    Error = std::convert::Infallible,
    Future = futures::future::Ready<Result<Response<HttpBody>, std::convert::Infallible>>,
> + Clone {
    tower::service_fn(|req: Request<HttpBody>| {
        let id = req
            .headers()
            .get(REQUEST_ID_HEADER)
            .map(|value| value.to_str().unwrap().to_string())
            .unwrap_or_default();
        let response = Response::builder()
            .status(StatusCode::OK)
            .body(HttpBody::new(Full::new(Bytes::from(id))))
            .expect("static body is infallible");
        futures::future::ready(Ok::<_, std::convert::Infallible>(response))
    })
}

fn request_with_header(value: Option<&str>) -> Request<HttpBody> {
    let mut builder = Request::builder().method(Method::POST).uri("/");
    if let Some(value) = value {
        builder = builder.header(REQUEST_ID_HEADER, value);
    }
    builder.body(HttpBody::new(Full::new(Bytes::new()))).expect("static body is infallible")
}

async fn read_body(response: Response<HttpBody>) -> (Vec<u8>, http::HeaderMap) {
    let (parts, body) = response.into_parts();
    let bytes = body.collect().await.expect("body collect").to_bytes().to_vec();
    (bytes, parts.headers)
}

#[tokio::test]
async fn echoes_supplied_request_id_on_response() {
    let svc = RequestLogLayer.layer(echo_request_id_service());

    let response = svc.oneshot(request_with_header(Some("client-supplied-id"))).await.unwrap();

    let (body, headers) = read_body(response).await;
    assert_eq!(headers.get(REQUEST_ID_HEADER).unwrap(), "client-supplied-id");
    assert_eq!(String::from_utf8(body).unwrap(), "client-supplied-id");
}

#[tokio::test]
async fn generates_request_id_when_absent_and_echoes_it() {
    let svc = RequestLogLayer.layer(echo_request_id_service());

    let response = svc.oneshot(request_with_header(None)).await.unwrap();

    let (body, headers) = read_body(response).await;
    let header_id = headers.get(REQUEST_ID_HEADER).unwrap().to_str().unwrap();
    let body_id = String::from_utf8(body).unwrap();
    // Body is what the inner service saw — they must match (the layer
    // rewrote the request header before forwarding).
    assert_eq!(header_id, body_id);
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

    let (_body, headers) = read_body(response).await;
    let header_id = headers.get(REQUEST_ID_HEADER).unwrap().to_str().unwrap();
    assert!(uuid::Uuid::parse_str(header_id).is_ok(), "should have generated a fresh UUID");
}

#[tokio::test]
async fn drops_request_id_containing_whitespace() {
    // CRLF in header values is rejected by the http crate itself at parse
    // time, so the residual concern is whitespace and other ASCII bytes
    // that would confuse log parsers if echoed verbatim into structured
    // fields.
    for hostile in ["with space", "tab\there", "leading space "] {
        let mut request = request_with_header(None);
        request
            .headers_mut()
            .insert(REQUEST_ID_HEADER, http::HeaderValue::from_bytes(hostile.as_bytes()).unwrap());
        let svc = RequestLogLayer.layer(echo_request_id_service());
        let response = svc.oneshot(request).await.unwrap();
        let (_body, headers) = read_body(response).await;
        let header_id = headers.get(REQUEST_ID_HEADER).unwrap().to_str().unwrap();
        assert!(
            uuid::Uuid::parse_str(header_id).is_ok(),
            "expected fresh UUID for hostile input {hostile:?}, got {header_id:?}",
        );
    }
}

#[tokio::test]
async fn drops_oversize_request_id() {
    let huge = "a".repeat(2048);
    let mut request = request_with_header(None);
    request
        .headers_mut()
        .insert(REQUEST_ID_HEADER, http::HeaderValue::from_bytes(huge.as_bytes()).unwrap());
    let svc = RequestLogLayer.layer(echo_request_id_service());
    let response = svc.oneshot(request).await.unwrap();
    let (_body, headers) = read_body(response).await;
    let header_id = headers.get(REQUEST_ID_HEADER).unwrap().to_str().unwrap();
    assert!(uuid::Uuid::parse_str(header_id).is_ok());
}
