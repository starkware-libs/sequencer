//! Shared fixtures for middleware-layer tests.

use std::time::Duration;

use bytes::Bytes;
use http::{HeaderMap, Method, Request, Response, StatusCode};
use http_body_util::{BodyExt, Full};
use jsonrpsee::server::HttpBody;

use crate::server::health::HealthLayer;
use crate::server::request_log::REQUEST_ID_HEADER;
use crate::server::saturation::SaturationMonitor;

/// `HealthLayer` over a fresh monitor with a zero threshold. No reject is ever recorded, so it
/// never reports saturation, and the zero threshold keeps sleeps out of tests.
pub fn unsaturated_health_layer() -> HealthLayer {
    HealthLayer::new(SaturationMonitor::default(), Duration::ZERO)
}

/// Inner service that echoes the request's `x-request-id` into the response
/// body, so tests can observe the id downstream layers saw.
pub fn echo_request_id_service() -> impl tower::Service<
    Request<HttpBody>,
    Response = Response<HttpBody>,
    Error = std::convert::Infallible,
    Future = futures::future::Ready<Result<Response<HttpBody>, std::convert::Infallible>>,
> + Clone {
    tower::service_fn(|request: Request<HttpBody>| {
        let id = request
            .headers()
            .get(REQUEST_ID_HEADER)
            .map(|value| value.to_str().expect("test ids are ASCII").to_string())
            .unwrap_or_default();
        let response = Response::builder()
            .status(StatusCode::OK)
            .body(HttpBody::new(Full::new(Bytes::from(id))))
            .expect("static body is infallible");
        futures::future::ready(Ok::<_, std::convert::Infallible>(response))
    })
}

/// Collects the response into its UTF-8 body string and headers.
pub async fn read_body_and_headers(response: Response<HttpBody>) -> (String, HeaderMap) {
    let (parts, body) = response.into_parts();
    let bytes = body.collect().await.expect("body collect").to_bytes();
    (String::from_utf8(bytes.to_vec()).expect("utf8 body"), parts.headers)
}

/// Inner stub returning 418, so a test can tell whether the layer under test
/// short-circuited the request or passed it through.
pub fn fallthrough_service() -> impl tower::Service<
    Request<HttpBody>,
    Response = Response<HttpBody>,
    Error = std::convert::Infallible,
    Future = futures::future::Ready<Result<Response<HttpBody>, std::convert::Infallible>>,
> + Clone {
    tower::service_fn(|_req: Request<HttpBody>| {
        let response = Response::builder()
            .status(StatusCode::IM_A_TEAPOT)
            .body(HttpBody::new(Full::new(Bytes::from_static(b"fallthrough"))))
            .expect("static body is infallible");
        futures::future::ready(Ok::<_, std::convert::Infallible>(response))
    })
}

/// Request with an empty body, for probing a path-matching layer.
pub fn empty_request(method: Method, path: &str) -> Request<HttpBody> {
    Request::builder()
        .method(method)
        .uri(path)
        .body(HttpBody::new(Full::new(Bytes::new())))
        .expect("static body is infallible")
}

/// Collects the response into its status, raw body bytes, and headers.
pub async fn read_response(response: Response<HttpBody>) -> (StatusCode, Vec<u8>, HeaderMap) {
    let (parts, body) = response.into_parts();
    let bytes = body.collect().await.expect("body collect").to_bytes().to_vec();
    (parts.status, bytes, parts.headers)
}
