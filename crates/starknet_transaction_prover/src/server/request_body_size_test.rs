//! Integration tests for the HTTP server's `max_request_body_size` enforcement.
//!
//! Verifies that the production default limit accepts realistically large
//! `starknet_proveTransaction` requests (5,000 full-width calldata felts) and that
//! jsonrpsee rejects bodies exceeding the configured limit with `413 Payload Too Large`.

use std::net::SocketAddr;
use std::sync::Arc;

use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use jsonrpsee::server::ServerHandle;
use reqwest::StatusCode;
use rstest::rstest;
use serde_json::{json, Value};
use starknet_api::invoke_tx_args;
use starknet_api::test_utils::invoke::rpc_invoke_tx;
use starknet_api::transaction::fields::Calldata;
use starknet_types_core::felt::Felt;

use crate::server::config::{TransportMode, DEFAULT_MAX_REQUEST_BODY_SIZE};
use crate::server::mock_rpc::MockProvingRpc;
use crate::server::rpc_api::ProvingRpcServer;
use crate::server::start_server;

const NUM_CALLDATA_FELTS: usize = 5_000;

async fn start_test_http_server(max_request_body_size: u32) -> (SocketAddr, ServerHandle) {
    let methods = MockProvingRpc::from_expected_json().into_rpc();
    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    start_server(addr, &TransportMode::Http, methods.into(), 10, max_request_body_size, None, None)
        .await
        .expect("Failed to start HTTP server")
}

/// A `starknet_proveTransaction` request whose invoke transaction carries `num_felts`
/// calldata felts. Each felt is `Felt::MAX` so it serializes at full hex width,
/// making the body the largest JSON encoding a calldata of that length can produce.
fn prove_transaction_request(num_felts: usize) -> Value {
    let transaction = rpc_invoke_tx(invoke_tx_args!(
        calldata: Calldata(Arc::new(vec![Felt::MAX; num_felts]))
    ));
    json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "starknet_proveTransaction",
        "params": { "block_id": BlockId::Latest, "transaction": transaction }
    })
}

async fn post_json_body(addr: SocketAddr, body: String) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!("http://{addr}"))
        .header("content-type", "application/json")
        .body(body)
        .send()
        .await
        .expect("HTTP request failed")
}

#[tokio::test]
async fn test_default_limit_accepts_5000_felt_calldata() {
    let (addr, handle) = start_test_http_server(DEFAULT_MAX_REQUEST_BODY_SIZE).await;

    let request_body = prove_transaction_request(NUM_CALLDATA_FELTS).to_string();
    // Guard that the request is genuinely large (~350 KiB) yet within the default limit,
    // so the assertions below keep their meaning if the encoding ever changes.
    assert!(
        request_body.len() > 300 * 1024,
        "expected a large request body, got {} bytes",
        request_body.len()
    );
    assert!(request_body.len() < usize::try_from(DEFAULT_MAX_REQUEST_BODY_SIZE).unwrap());

    let response = post_json_body(addr, request_body).await;
    assert_eq!(response.status(), StatusCode::OK);
    let response_json: Value = response.json().await.unwrap();
    assert!(
        response_json.get("error").is_none() && response_json.get("result").is_some(),
        "expected a JSON-RPC success, got: {response_json}"
    );

    handle.stop().unwrap();
}

/// `max_request_body_size` is inclusive: a body of exactly the configured size is
/// served, one byte more is rejected with HTTP 413. The body is padded with trailing
/// spaces (valid JSON whitespace) to hit exact byte counts.
#[rstest]
#[case::body_at_limit(0, StatusCode::OK)]
#[case::body_one_byte_over_limit(1, StatusCode::PAYLOAD_TOO_LARGE)]
#[tokio::test]
async fn test_body_size_limit_boundary(
    #[case] num_bytes_over_limit: usize,
    #[case] expected_status: StatusCode,
) {
    let unpadded_body = prove_transaction_request(NUM_CALLDATA_FELTS).to_string();
    let body_size_limit = u32::try_from(unpadded_body.len()).unwrap();
    let (addr, handle) = start_test_http_server(body_size_limit).await;

    let padded_body = format!("{}{}", unpadded_body, " ".repeat(num_bytes_over_limit));
    let response = post_json_body(addr, padded_body).await;
    assert_eq!(response.status(), expected_status);

    handle.stop().unwrap();
}
