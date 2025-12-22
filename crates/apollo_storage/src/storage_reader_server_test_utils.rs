//! Test utilities for storage reader server testing.

use axum::body::{Body, Bytes, HttpBody};
use axum::http::Request;
use axum::response::Response;
use axum::Router;
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
