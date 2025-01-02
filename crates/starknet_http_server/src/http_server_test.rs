use axum::body::{Bytes, HttpBody};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use starknet_api::transaction::TransactionHash;

use crate::http_server::add_tx_result_as_json;

#[tokio::test]
async fn test_tx_hash_json_conversion() {
    let tx_hash = TransactionHash::default();
    let response = add_tx_result_as_json(Ok(tx_hash)).into_response();

    let status_code = response.status();
    let response_bytes = &to_bytes(response).await;

    assert_eq!(status_code, StatusCode::OK, "{response_bytes:?}");
    assert_eq!(tx_hash, serde_json::from_slice(response_bytes).unwrap());
}

async fn to_bytes(res: Response) -> Bytes {
    res.into_body().collect().await.unwrap().to_bytes()
}
