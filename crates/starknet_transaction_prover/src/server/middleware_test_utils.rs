//! Shared fixtures for middleware-layer tests.

use bytes::Bytes;
use http::{HeaderMap, Request, Response, StatusCode};
use http_body_util::{BodyExt, Full};
use jsonrpsee::server::HttpBody;

use crate::server::request_log::REQUEST_ID_HEADER;

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
