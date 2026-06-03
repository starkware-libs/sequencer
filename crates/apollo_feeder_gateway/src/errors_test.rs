use axum::body::to_bytes;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use starknet_api::block::BlockNumber;

use crate::errors::FeederGatewayError;

async fn response_status_and_body(error: FeederGatewayError) -> (StatusCode, String) {
    let response = error.into_response();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

#[tokio::test]
async fn block_not_found_envelope_is_byte_parity() {
    // Live format: get_block?blockNumber=999999999 echoes the number (verified 2026-06-03).
    let (status, body) =
        response_status_and_body(FeederGatewayError::BlockNotFound(BlockNumber(999999999))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body,
        r#"{"code": "StarknetErrorCode.BLOCK_NOT_FOUND", "message": "Block number 999999999 was not found."}"#
    );
}

#[tokio::test]
async fn block_hash_not_found_envelope_is_byte_parity() {
    // Live format: get_block_id_by_hash?blockHash=0xdeadbeef echoes the raw hash string.
    let (status, body) =
        response_status_and_body(FeederGatewayError::BlockHashNotFound("0xdeadbeef".to_string()))
            .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body,
        r#"{"code": "StarknetErrorCode.BLOCK_NOT_FOUND", "message": "Block hash 0xdeadbeef does not exist."}"#
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
async fn malformed_request_echoes_message_verbatim() {
    // The live feeder gateway echoes request values verbatim with JSON escaping only (verified
    // live: blockHash=zz";<>&x echoes as zz\";<>&x), so no sanitization is applied.
    let (status, body) = response_status_and_body(FeederGatewayError::MalformedRequest(
        "got: zz\";<>&x.".to_string(),
    ))
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body,
        r#"{"code": "StarkErrorCode.MALFORMED_REQUEST", "message": "got: zz\";<>&x."}"#
    );
}
