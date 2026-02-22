//! Test utilities for storage reader server testing.

use std::net::SocketAddr;

use axum::body::{Body, Bytes};
use axum::http::{Request, StatusCode};
use axum::response::Response;
use axum::Router;
use http_body_util::BodyExt;
use serde::de::DeserializeOwned;
use serde::Serialize;
use tower::util::ServiceExt;

const STORAGE_QUERY_PATH: &str = "/storage/query";

/// Helper function to send a storage query request.
pub async fn send_storage_query<T: Serialize>(app: Router, request: &T) -> Response {
    app.oneshot(
        Request::builder()
            .method("POST")
            .uri(STORAGE_QUERY_PATH)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(request).unwrap()))
            .unwrap(),
    )
    .await
    .unwrap()
}

/// Helper function to convert response body to bytes.
pub async fn to_bytes(res: Response) -> Bytes {
    res.into_body().collect().await.unwrap().to_bytes()
}

/// Helper function to send a storage query request and get the deserialized response.
/// Asserts that the response matches the expected status code.
pub async fn get_response<Req: Serialize, Res: DeserializeOwned>(
    app: Router,
    request: &Req,
    expected_status: StatusCode,
) -> Res {
    let response = send_storage_query(app, request).await;
    assert_eq!(response.status(), expected_status);
    let body = to_bytes(response).await;
    serde_json::from_slice(&body).unwrap()
}

/// Sends an HTTP request to a running storage reader server and returns the deserialized response.
pub async fn send_storage_reader_http_request<Req: Serialize, Res: DeserializeOwned>(
    addr: SocketAddr,
    request: &Req,
) -> Res {
    let url = format!("http://{addr}{STORAGE_QUERY_PATH}");
    let response = reqwest::Client::new()
        .post(&url)
        .json(request)
        .send()
        .await
        .expect("Failed to send request to storage reader server");
    assert!(response.status().is_success(), "Storage reader server returned error");
    response.json().await.expect("Failed to parse storage reader server response")
}
