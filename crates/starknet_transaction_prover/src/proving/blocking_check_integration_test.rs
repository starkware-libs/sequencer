//! Integration tests for the sequential blocking check in `prove_transaction`.
//!
//! Uses a mock `VirtualSnosRunner` combined with a `mockito` server for the
//! external check endpoint. The mock runner always returns an error — this avoids
//! needing the `stwo_proving` feature while still exercising the blocking check
//! integration points.
//!
//! The key distinction: when the blocking check short-circuits, we get
//! `TransactionBlocked`; when proving proceeds, we get `RunnerError`.

use async_trait::async_trait;
use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use mockito::Server;
use serde_json::{json, Map, Value};
use starknet_api::invoke_tx_args;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::test_utils::invoke::rpc_invoke_tx;
use starknet_api::transaction::fields::ValidResourceBounds;
use starknet_api::transaction::InvokeTransaction;
use url::Url;

use crate::blocking_check::BlockingCheckClient;
use crate::errors::{RunnerError, VirtualSnosProverError};
use crate::proving::virtual_snos_prover::{ProveTransactionResult, VirtualSnosProver};
use crate::running::runner::{RunnerOutput, VirtualSnosRunner};
use crate::test_utils::resource_bounds_for_client_side_tx;

/// A mock runner that always returns an error.
///
/// Returns `RunnerError::InputGenerationError` to avoid needing the `stwo_proving`
/// feature. Tests distinguish blocking check outcomes by checking for
/// `TransactionBlocked` vs `RunnerError`.
#[derive(Clone)]
struct MockRunner;

#[async_trait]
impl VirtualSnosRunner for MockRunner {
    async fn run_virtual_os(
        &self,
        _block_id: BlockId,
        _txs: Vec<InvokeTransaction>,
    ) -> Result<RunnerOutput, RunnerError> {
        Err(RunnerError::InputGenerationError("mock runner".to_string()))
    }
}

/// Default timeout used by tests that expect the check to complete within the timeout.
const TEST_TIMEOUT_MILLIS: u64 = 10_000;

fn test_transaction() -> RpcTransaction {
    rpc_invoke_tx(invoke_tx_args!(
        resource_bounds: ValidResourceBounds::AllResources(resource_bounds_for_client_side_tx())
    ))
}

/// Helper to build a prover with a mock runner and optional blocking check client.
fn build_prover(
    runner: MockRunner,
    blocking_check_client: Option<BlockingCheckClient>,
) -> VirtualSnosProver<MockRunner> {
    VirtualSnosProver::from_runner_with_blocking_check(runner, blocking_check_client)
}

/// Asserts the result is a RunnerError (proving proceeded, was not blocked).
fn assert_runner_error(result: &Result<(), VirtualSnosProverError>) {
    assert!(
        matches!(result, Err(VirtualSnosProverError::RunnerError(_))),
        "Expected RunnerError (proving proceeded), got: {result:?}"
    );
}

/// Asserts the result is TransactionBlocked (blocking check short-circuited).
fn assert_blocked(result: &Result<(), VirtualSnosProverError>) {
    assert!(
        matches!(result, Err(VirtualSnosProverError::TransactionBlocked)),
        "Expected TransactionBlocked, got: {result:?}"
    );
}

/// Maps the result to strip the success type for easier assertion.
async fn prove(prover: &VirtualSnosProver<MockRunner>) -> Result<(), VirtualSnosProverError> {
    prover.prove_transaction(BlockId::Latest, test_transaction()).await.map(|_| ())
}

#[tokio::test]
async fn test_no_blocking_check_configured_proceeds_to_proving() {
    let prover = build_prover(MockRunner, None);
    let result = prove(&prover).await;
    assert_runner_error(&result);
}

#[tokio::test]
async fn test_check_blocked_returns_blocked() {
    let mut server = Server::new_async().await;
    let _mock = server
        .mock("POST", "/")
        .with_status(200)
        .with_body(r#"{"jsonrpc":"2.0","error":{"code":10000,"message":"Blocked"},"id":1}"#)
        .create_async()
        .await;

    let url = Url::parse(&server.url()).unwrap();
    let client = BlockingCheckClient::new(url, TEST_TIMEOUT_MILLIS, true);
    let prover = build_prover(MockRunner, Some(client));

    let result = prove(&prover).await;
    assert_blocked(&result);
}

#[tokio::test]
async fn test_check_allowed_proceeds_to_proving() {
    let mut server = Server::new_async().await;
    let _mock = server
        .mock("POST", "/")
        .with_status(200)
        .with_body(r#"{"jsonrpc":"2.0","result":{},"id":1}"#)
        .create_async()
        .await;

    let url = Url::parse(&server.url()).unwrap();
    let client = BlockingCheckClient::new(url, TEST_TIMEOUT_MILLIS, true);
    let prover = build_prover(MockRunner, Some(client));

    let result = prove(&prover).await;
    assert_runner_error(&result);
}

#[tokio::test]
async fn test_check_allowed_with_additional_data_proceeds_to_proving() {
    let mut server = Server::new_async().await;
    let _mock = server
        .mock("POST", "/")
        .with_status(200)
        .with_body(
            r#"{"jsonrpc":"2.0","result":{"allowed":true,"additional_data":{"signature":{"issued_at":1716579600,"sig_r":"0x1","sig_s":"0x2"}}},"id":1}"#,
        )
        .create_async()
        .await;

    let url = Url::parse(&server.url()).unwrap();
    let client = BlockingCheckClient::new(url, TEST_TIMEOUT_MILLIS, true);
    let prover = build_prover(MockRunner, Some(client));

    // The MockRunner errors before producing a result, so the verbatim relay is
    // covered by the serde tests below; this confirms an allow carrying
    // additional_data routes to proving rather than blocking.
    let result = prove(&prover).await;
    assert_runner_error(&result);
}

fn sample_additional_data() -> Map<String, Value> {
    serde_json::from_value(json!({
        "signature": { "issued_at": 1716579600, "sig_r": "0x6e6f63c8", "sig_s": "0x58a68a71" }
    }))
    .unwrap()
}

#[test]
fn test_additional_data_relays_object_verbatim() {
    // The prover does not interpret additional_data; an arbitrary object,
    // including keys it has never heard of, round-trips unchanged.
    let additional_data: Map<String, Value> =
        serde_json::from_value(json!({ "signature": { "sig_r": "0x1" }, "future_key": [1, 2, 3] }))
            .unwrap();
    let fixture = include_str!("../../resources/mock_proving_rpc/prove_transaction_result.json");
    let mut result: ProveTransactionResult = serde_json::from_str(fixture).unwrap();
    result.additional_data = Some(additional_data.clone());

    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["additional_data"], Value::Object(additional_data));
}

#[test]
fn test_empty_additional_data_is_omitted_from_prove_result_json() {
    // The committed mock fixture carries no additional_data; the field must
    // deserialize to `None` and stay absent on re-serialization.
    let fixture = include_str!("../../resources/mock_proving_rpc/prove_transaction_result.json");
    let result: ProveTransactionResult = serde_json::from_str(fixture).unwrap();
    assert!(result.additional_data.is_none());

    let json = serde_json::to_value(&result).unwrap();
    // Must be absent, not serialized as `null`: `contains_key` is true for an explicit null.
    assert!(!json.as_object().unwrap().contains_key("additional_data"));
}

#[test]
fn test_populated_additional_data_is_present_in_prove_result_json() {
    let fixture = include_str!("../../resources/mock_proving_rpc/prove_transaction_result.json");
    let mut result: ProveTransactionResult = serde_json::from_str(fixture).unwrap();
    result.additional_data = Some(sample_additional_data());

    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["additional_data"]["signature"]["sig_r"], "0x6e6f63c8");
}

#[tokio::test]
async fn test_inconclusive_fail_open_proceeds_to_proving() {
    let mut server = Server::new_async().await;
    let _mock = server
        .mock("POST", "/")
        .with_status(200)
        .with_body(r#"{"jsonrpc":"2.0","error":{"code":-32603,"message":"Internal error"},"id":1}"#)
        .create_async()
        .await;

    let url = Url::parse(&server.url()).unwrap();
    let client = BlockingCheckClient::new(url, TEST_TIMEOUT_MILLIS, true);
    let prover = build_prover(MockRunner, Some(client));

    let result = prove(&prover).await;
    assert_runner_error(&result);
}

#[tokio::test]
async fn test_inconclusive_fail_close_returns_blocked() {
    let mut server = Server::new_async().await;
    let _mock = server
        .mock("POST", "/")
        .with_status(200)
        .with_body(r#"{"jsonrpc":"2.0","error":{"code":-32603,"message":"Internal error"},"id":1}"#)
        .create_async()
        .await;

    let url = Url::parse(&server.url()).unwrap();
    let client = BlockingCheckClient::new(url, TEST_TIMEOUT_MILLIS, false);
    let prover = build_prover(MockRunner, Some(client));

    let result = prove(&prover).await;
    assert_blocked(&result);
}

#[tokio::test]
async fn test_timeout_fail_open_proceeds_to_proving() {
    // Zero timeout + hanging URL forces the timeout path without real-time waits.
    let url = Url::parse("http://10.255.255.1:1").unwrap();
    let client = BlockingCheckClient::new(url, 0, true);
    let prover = build_prover(MockRunner, Some(client));

    let result = prove(&prover).await;
    assert_runner_error(&result);
}

#[tokio::test]
async fn test_timeout_fail_close_returns_blocked() {
    // Zero timeout + hanging URL forces the timeout path without real-time waits.
    let url = Url::parse("http://10.255.255.1:1").unwrap();
    let client = BlockingCheckClient::new(url, 0, false);
    let prover = build_prover(MockRunner, Some(client));

    let result = prove(&prover).await;
    assert_blocked(&result);
}
