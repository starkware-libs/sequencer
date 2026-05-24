use http::{Method, StatusCode};
use tower::{Layer, ServiceExt};

use crate::server::health::{HealthLayer, HEALTH_PATH};
use crate::server::middleware_test_utils::{empty_request, fallthrough_service, read_response};

#[tokio::test]
async fn get_health_returns_200_with_json_body() {
    let svc = HealthLayer.layer(fallthrough_service());

    let response = svc.oneshot(empty_request(Method::GET, HEALTH_PATH)).await.unwrap();

    let (status, body, headers) = read_response(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, br#"{"status":"ok"}"#);
    assert_eq!(headers.get(http::header::CONTENT_TYPE).unwrap(), "application/json");
}

#[tokio::test]
async fn non_get_health_falls_through() {
    let svc = HealthLayer.layer(fallthrough_service());

    let response = svc.oneshot(empty_request(Method::POST, HEALTH_PATH)).await.unwrap();

    let (status, _body, _) = read_response(response).await;
    assert_eq!(status, StatusCode::IM_A_TEAPOT);
}

#[tokio::test]
async fn get_other_path_falls_through() {
    let svc = HealthLayer.layer(fallthrough_service());

    let response = svc.oneshot(empty_request(Method::GET, "/")).await.unwrap();

    let (status, _body, _) = read_response(response).await;
    assert_eq!(status, StatusCode::IM_A_TEAPOT);
}
