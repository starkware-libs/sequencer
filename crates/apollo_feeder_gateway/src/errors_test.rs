use axum::body::to_bytes;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use crate::errors::FeederGatewayError;

async fn response_status_and_body(error: FeederGatewayError) -> (StatusCode, String) {
    let response = error.into_response();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

#[tokio::test]
async fn block_not_found_envelope_is_byte_parity() {
    let (status, body) = response_status_and_body(FeederGatewayError::BlockNotFound).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body,
        r#"{"code": "StarknetErrorCode.BLOCK_NOT_FOUND", "message": "Block not found"}"#
    );
}

#[tokio::test]
async fn transaction_not_found_envelope_is_byte_parity() {
    let (status, body) = response_status_and_body(FeederGatewayError::TransactionNotFound).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body,
        r#"{"code": "StarknetErrorCode.TRANSACTION_NOT_FOUND", "message": "Transaction hash not found"}"#
    );
}

#[tokio::test]
async fn internal_envelope_is_byte_parity_and_leaks_nothing() {
    let (status, body) = response_status_and_body(FeederGatewayError::Internal).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        body,
        r#"{"code": "StarknetErrorCode.INTERNAL_ERROR", "message": "Internal error"}"#
    );
}

#[tokio::test]
async fn malformed_request_sanitizes_message() {
    let (status, body) =
        response_status_and_body(FeederGatewayError::MalformedRequest("bad \"x\" <y>".to_string()))
            .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    // Quotes become single quotes and disallowed characters (`<`, `>`) become spaces.
    assert_eq!(body, r#"{"code": "StarkErrorCode.MALFORMED_REQUEST", "message": "bad 'x'  y "}"#);
}
