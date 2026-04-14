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
use starknet_api::invoke_tx_args;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::test_utils::invoke::rpc_invoke_tx;
use starknet_api::transaction::fields::{AllResourceBounds, ValidResourceBounds};
use starknet_api::transaction::InvokeTransaction;
use url::Url;

use super::virtual_snos_prover::VirtualSnosProver;
use crate::blocking_check::BlockingCheckClient;
use crate::errors::{RunnerError, VirtualSnosProverError};
use crate::running::runner::{RunnerOutput, VirtualSnosRunner};

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
        resource_bounds: ValidResourceBounds::AllResources(AllResourceBounds::default())
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
