//! Test utilities for storage reader server testing.

use axum::body::{Body, Bytes, HttpBody};
use axum::http::{Request, StatusCode};
use axum::response::Response;
use axum::Router;
use serde::de::DeserializeOwned;
use serde::Serialize;
use tower::ServiceExt;

/// Helper function to send a storage query request.
pub async fn send_storage_query<T: Serialize>(app: Router, request: &T) -> Response {
    app.oneshot(
        Request::builder()
            .method("POST")
            .uri("/storage/query")
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
