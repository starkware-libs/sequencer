//! Client for the external blocking check JSON-RPC service.
//!
//! Sends `starknet_checkTransaction` requests in parallel with proving
//! to determine whether a transaction should be blocked.

use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use serde::{Deserialize, Serialize};
use starknet_api::rpc_transaction::RpcTransaction;
use tracing::warn;
use url::Url;

#[cfg(test)]
#[path = "blocking_check_test.rs"]
mod blocking_check_test;

/// Error code returned by the external service to signal a blocked transaction.
const BLOCKED_ERROR_CODE: i32 = 10000;

/// Result of a blocking check call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BlockingCheckResult {
    /// External service returned success — transaction is allowed.
    Allowed,
    /// External service returned error code 10000 — transaction is blocked.
    Blocked,
    /// Any other outcome (network error, non-10000 error, deserialization failure).
    Inconclusive,
}

/// Configuration and HTTP client for the external blocking check service.
#[derive(Clone)]
pub(crate) struct BlockingCheckClient {
    http_client: reqwest::Client,
    url: Url,
    pub(crate) timeout_secs: u64,
    pub(crate) fail_open: bool,
}

/// JSON-RPC request body.
#[derive(Serialize)]
struct JsonRpcRequest {
    jsonrpc: &'static str,
    method: &'static str,
    params: CheckTransactionParams,
    id: u64,
}

/// Parameters for starknet_checkTransaction (same as starknet_proveTransaction).
#[derive(Serialize)]
struct CheckTransactionParams {
    block_id: BlockId,
    transaction: RpcTransaction,
}

/// JSON-RPC response envelope (only the fields we need).
#[derive(Deserialize)]
struct JsonRpcResponse {
    #[serde(default)]
    error: Option<JsonRpcError>,
}

/// JSON-RPC error object (only the code field is needed for decision logic).
#[derive(Deserialize)]
struct JsonRpcError {
    code: i32,
}

impl BlockingCheckClient {
    /// Creates a new blocking check client.
    ///
    /// The HTTP client is configured to accept self-signed TLS certificates.
    pub(crate) fn new(url: Url, timeout_secs: u64, fail_open: bool) -> Self {
        let http_client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .expect("Failed to build blocking check HTTP client");
        Self { http_client, url, timeout_secs, fail_open }
    }

    /// Sends `starknet_checkTransaction` to the external service and interprets the response.
    pub(crate) async fn check_transaction(
        &self,
        block_id: BlockId,
        transaction: RpcTransaction,
    ) -> BlockingCheckResult {
        let request_body = JsonRpcRequest {
            jsonrpc: "2.0",
            method: "starknet_checkTransaction",
            params: CheckTransactionParams { block_id, transaction },
            id: 1,
        };

        let response =
            match self.http_client.post(self.url.as_str()).json(&request_body).send().await {
                Ok(resp) => resp,
                Err(err) => {
                    warn!("Blocking check request failed: {err}");
                    return BlockingCheckResult::Inconclusive;
                }
            };

        let body = match response.text().await {
            Ok(text) => text,
            Err(err) => {
                warn!("Failed to read blocking check response body: {err}");
                return BlockingCheckResult::Inconclusive;
            }
        };

        let json_rpc_response: JsonRpcResponse = match serde_json::from_str(&body) {
            Ok(parsed) => parsed,
            Err(err) => {
                warn!("Failed to parse blocking check response: {err}");
                return BlockingCheckResult::Inconclusive;
            }
        };

        match json_rpc_response.error {
            None => BlockingCheckResult::Allowed,
            Some(err) if err.code == BLOCKED_ERROR_CODE => BlockingCheckResult::Blocked,
            Some(err) => {
                warn!("Blocking check returned non-blocking error code: {}", err.code);
                BlockingCheckResult::Inconclusive
            }
        }
    }
}
