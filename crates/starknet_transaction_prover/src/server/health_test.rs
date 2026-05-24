use bytes::Bytes;
use http::{Method, Request, Response, StatusCode};
use http_body_util::{BodyExt, Full};
use jsonrpsee::server::HttpBody;
use tower::{Layer, ServiceExt};

use crate::server::health::{HealthLayer, HEALTH_PATH};

/// Inner stub returning 418 so we can tell whether `HealthLayer` short-circuited.
fn fallthrough_service() -> impl tower::Service<
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

fn empty_request(method: Method, path: &str) -> Request<HttpBody> {
    Request::builder()
        .method(method)
        .uri(path)
        .body(HttpBody::new(Full::new(Bytes::new())))
        .expect("static body is infallible")
}

async fn read_body(response: Response<HttpBody>) -> (StatusCode, Vec<u8>, http::HeaderMap) {
    let (parts, body) = response.into_parts();
    let bytes = body.collect().await.expect("body collect").to_bytes().to_vec();
    (parts.status, bytes, parts.headers)
}

#[tokio::test]
async fn get_health_returns_200_with_json_body() {
    let svc = HealthLayer.layer(fallthrough_service());

    let response = svc.oneshot(empty_request(Method::GET, HEALTH_PATH)).await.unwrap();

    let (status, body, headers) = read_body(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, br#"{"status":"ok"}"#);
    assert_eq!(headers.get(http::header::CONTENT_TYPE).unwrap(), "application/json");
}

#[tokio::test]
async fn non_get_health_falls_through() {
    let svc = HealthLayer.layer(fallthrough_service());

    let response = svc.oneshot(empty_request(Method::POST, HEALTH_PATH)).await.unwrap();

    let (status, _body, _) = read_body(response).await;
    assert_eq!(status, StatusCode::IM_A_TEAPOT);
}

#[tokio::test]
async fn get_other_path_falls_through() {
    let svc = HealthLayer.layer(fallthrough_service());

    let response = svc.oneshot(empty_request(Method::GET, "/")).await.unwrap();

    let (status, _body, _) = read_body(response).await;
    assert_eq!(status, StatusCode::IM_A_TEAPOT);
}
