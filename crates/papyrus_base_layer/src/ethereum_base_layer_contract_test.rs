use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use alloy::providers::mock::{Asserter, MockTransport};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::client::RpcClient;
use alloy::rpc::json_rpc::{ErrorPayload, RequestPacket, ResponsePacket, RpcError};
use alloy::rpc::types::Log;
use alloy::transports::{HttpError, TransportErrorKind, TransportFut};
use tokio::time::error::Elapsed;
use tower::Service;
use url::Url;

use crate::ethereum_base_layer_contract::{
    is_range_too_large_error,
    EthereumBaseLayerConfig,
    EthereumBaseLayerContract,
    EthereumBaseLayerError,
};
use crate::BaseLayerContract;

fn base_layer_with_mocked_provider() -> (EthereumBaseLayerContract, Asserter) {
    // The Asserter is a FIFO queue of mocked responses (success or failure).
    let asserter = Asserter::new();
    let provider = ProviderBuilder::new().connect_mocked_client(asserter.clone()).root().clone();
    let base_layer =
        EthereumBaseLayerContract::new_with_provider(EthereumBaseLayerConfig::default(), provider);
    (base_layer, asserter)
}

/// Transport whose first request hangs past the configured timeout (so the production
/// `tokio::time::timeout` fires a real `Elapsed`, exercising the timeout-triggered bisection path);
/// every later request delegates to the wrapped `Asserter` queue immediately.
#[derive(Clone)]
struct HangOnFirstRequestTransport {
    inner: MockTransport,
    first_request_seen: Arc<AtomicBool>,
}

impl Service<RequestPacket> for HangOnFirstRequestTransport {
    type Response = ResponsePacket;
    type Error = alloy::transports::TransportError;
    type Future = TransportFut<'static>;

    fn poll_ready(&mut self, _context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: RequestPacket) -> Self::Future {
        let is_first_request = !self.first_request_seen.swap(true, Ordering::SeqCst);
        let mut inner = self.inner.clone();
        Box::pin(async move {
            // Hang so the outer timeout elapses; the future is then dropped without touching the
            // Asserter queue, leaving the half-range responses for the retries.
            if is_first_request {
                std::future::pending::<()>().await;
            }
            inner.call(request).await
        })
    }
}

// Timeout short enough to fire promptly against the hanging first request, but long enough that the
// immediate half-range retries never race it.
const HANGING_PROVIDER_TIMEOUT: Duration = Duration::from_millis(50);

fn base_layer_hanging_on_first_request() -> (EthereumBaseLayerContract, Asserter) {
    let asserter = Asserter::new();
    let transport = HangOnFirstRequestTransport {
        inner: MockTransport::new(asserter.clone()),
        first_request_seen: Arc::new(AtomicBool::new(false)),
    };
    let provider =
        ProviderBuilder::new().connect_client(RpcClient::new(transport, true)).root().clone();
    let config =
        EthereumBaseLayerConfig { timeout_millis: HANGING_PROVIDER_TIMEOUT, ..Default::default() };
    let base_layer = EthereumBaseLayerContract::new_with_provider(config, provider);
    (base_layer, asserter)
}

fn too_many_results_error() -> ErrorPayload {
    ErrorPayload {
        code: -32005,
        message: "query returned more than 10000 results".into(),
        data: None,
    }
}

async fn make_elapsed() -> Elapsed {
    tokio::time::timeout(Duration::ZERO, std::future::pending::<()>()).await.unwrap_err()
}

fn log_at_block(block_number: u64) -> Log {
    Log { block_number: Some(block_number), ..Default::default() }
}

#[tokio::test]
#[ignore = "This test uses external dependencies, like Infura. But still it is a good \
            reference/sanity check."]
async fn fusaka_blob_fee_sanity_check() {
    let mut config = EthereumBaseLayerConfig {
        fusaka_no_bpo_start_block_number: 0,
        bpo1_start_block_number: 0,
        bpo2_start_block_number: 0,
        timeout_millis: Duration::from_millis(5000),
        ..Default::default()
    };

    // Timeline: Sepolia went on Fusaka on epoch 272640 (slot 8724480) which is about block 9408577
    // It went on BPO1 on epoch 274176 (slot 8773632) which is about block 9456501
    // It went on BPO2 on epoch 275712 (slot 8822784) which is about block 9504747
    let infura_api_key = std::env::var("INFURA_API_KEY")
        .expect("expected infura api key to be set in INFURA_API_KEY environment variable");
    let url = Url::parse(&format!("https://sepolia.infura.io/v3/{}", infura_api_key))
        .expect("expected infura url to be valid");
    config.ordered_l1_endpoint_urls = vec![url.into()];
    let mut base_layer = EthereumBaseLayerContract::new(config.clone());

    // This is a known time when the data gas price was relatively high:
    // https://sepolia.blobscan.com/block/9716185
    // The blob fee here is 0.010629722 wei.
    let block_number = 9716185;
    let base_fee_from_blobscan = 10629722;
    let block_header = base_layer
        .get_block_header(block_number)
        .await
        .expect("expected call to get block header to succeed")
        .expect("expected block header to be found");

    assert_eq!(block_header.blob_fee, base_fee_from_blobscan);

    // Now try to unset the fusaka configuration, to see if we get a massively bigger blob fee.
    base_layer.config.fusaka_no_bpo_start_block_number = 100000000;
    base_layer.config.bpo1_start_block_number = 1000000000;
    base_layer.config.bpo2_start_block_number = 1000000000;
    let block_header = base_layer
        .get_block_header(block_number)
        .await
        .expect("expected call to get block header to succeed")
        .expect("expected block header to be found");

    assert!(block_header.blob_fee > 1000 * base_fee_from_blobscan);

    // Choose a mainnet block number that is not yet on Fusaka (but has non-zero blob fee).
    // https://blobscan.com/block/23824000
    // The blob fee here is 31.042082881 Gwei.
    let url = Url::parse(&format!("https://mainnet.infura.io/v3/{}", infura_api_key))
        .expect("expected infura url to be valid");
    config.ordered_l1_endpoint_urls = vec![url.into()];
    let mut base_layer = EthereumBaseLayerContract::new(config);
    base_layer.config.fusaka_no_bpo_start_block_number = 100000000;
    base_layer.config.bpo1_start_block_number = 1000000000;
    base_layer.config.bpo2_start_block_number = 1000000000;
    let block_number = 23824000;
    let base_fee_from_blobscan = 31042082881;
    let block_header = base_layer
        .get_block_header(block_number)
        .await
        .expect("expected call to get block header to succeed")
        .expect("expected block header to be found");
    assert_eq!(block_header.blob_fee, base_fee_from_blobscan);

    // But if we set the fusaka update to have already happened, we should get a much lower fee.
    base_layer.config.fusaka_no_bpo_start_block_number = 0;
    base_layer.config.bpo1_start_block_number = 0;
    base_layer.config.bpo2_start_block_number = 0;
    let block_header = base_layer
        .get_block_header(block_number)
        .await
        .expect("expected call to get block header to succeed")
        .expect("expected block header to be found");
    assert!(block_header.blob_fee * 1000 < base_fee_from_blobscan);
}

#[tokio::test]
async fn events_single_call_success_is_unaffected() {
    let (mut base_layer, asserter) = base_layer_with_mocked_provider();
    asserter.push_success(&Vec::<Log>::new());

    let events = base_layer.events(1..=100, &[]).await.unwrap();

    assert!(events.is_empty());
}

#[tokio::test]
async fn events_bisects_on_too_many_results_and_drains_queue() {
    let (mut base_layer, asserter) = base_layer_with_mocked_provider();
    // Full range fails as oversized, then each half returns empty logs.
    asserter.push_failure(too_many_results_error());
    asserter.push_success(&Vec::<Log>::new());
    asserter.push_success(&Vec::<Log>::new());

    let events = base_layer.events(1..=4, &[]).await.unwrap();

    assert!(events.is_empty());
    // 1 failed full-range call + 2 successful half-range calls must have drained the queue; a
    // fourth call would panic on the empty asserter, proving exactly two halves were fetched.
    asserter.push_success(&Vec::<Log>::new());
    base_layer.events(1..=1, &[]).await.unwrap();
}

#[tokio::test]
async fn events_recursively_bisects_down_to_single_blocks() {
    let (mut base_layer, asserter) = base_layer_with_mocked_provider();
    // 1..=4 fails -> [1..=2, 3..=4]. 1..=2 fails -> [1..=1, 2..=2] (both empty). 3..=4 fails ->
    // [3..=3, 4..=4] (both empty). Low-before-high traversal fixes this exact response order.
    asserter.push_failure(too_many_results_error());
    asserter.push_failure(too_many_results_error());
    asserter.push_success(&Vec::<Log>::new());
    asserter.push_success(&Vec::<Log>::new());
    asserter.push_failure(too_many_results_error());
    asserter.push_success(&Vec::<Log>::new());
    asserter.push_success(&Vec::<Log>::new());

    let events = base_layer.events(1..=4, &[]).await.unwrap();

    assert!(events.is_empty());
}

#[tokio::test]
async fn bisection_returns_all_logs_in_ascending_block_order() {
    let (base_layer, asserter) = base_layer_with_mocked_provider();
    // Full range fails as oversized -> [1..=2, 3..=4]; the low half is fetched first, so its logs
    // (blocks 1, 2) must precede the high half's logs (blocks 3, 4) in the concatenated result.
    asserter.push_failure(too_many_results_error());
    asserter.push_success(&vec![log_at_block(1), log_at_block(2)]);
    asserter.push_success(&vec![log_at_block(3), log_at_block(4)]);

    let logs = base_layer.get_logs_bisected(1..=4, &[]).await.unwrap();

    let block_numbers: Vec<_> = logs.iter().map(|log| log.block_number).collect();
    assert_eq!(block_numbers, vec![Some(1), Some(2), Some(3), Some(4)]);
}

#[tokio::test]
async fn bisection_on_timeout_returns_all_logs_in_ascending_block_order() {
    let (base_layer, asserter) = base_layer_hanging_on_first_request();
    // The wide-range query hangs and times out (classified as range-too-large) -> [1..=2, 3..=4];
    // the low half is fetched first, so its logs (blocks 1, 2) must precede the high half's logs
    // (blocks 3, 4) in the concatenated result.
    asserter.push_success(&vec![log_at_block(1), log_at_block(2)]);
    asserter.push_success(&vec![log_at_block(3), log_at_block(4)]);

    let logs = base_layer.get_logs_bisected(1..=4, &[]).await.unwrap();

    let block_numbers: Vec<_> = logs.iter().map(|log| log.block_number).collect();
    assert_eq!(block_numbers, vec![Some(1), Some(2), Some(3), Some(4)]);
}

#[tokio::test]
async fn events_single_block_floor_still_failing_propagates() {
    let (mut base_layer, asserter) = base_layer_with_mocked_provider();
    asserter.push_failure(too_many_results_error());

    let error = base_layer.events(5..=5, &[]).await.unwrap_err();

    assert_eq!(
        error,
        EthereumBaseLayerError::RpcError(RpcError::ErrorResp(too_many_results_error()))
    );
}

#[tokio::test]
async fn events_propagates_non_range_error_without_bisecting() {
    let (mut base_layer, asserter) = base_layer_with_mocked_provider();
    let auth_error = ErrorPayload { code: -32000, message: "unauthorized".into(), data: None };
    asserter.push_failure(auth_error.clone());

    let error = base_layer.events(1..=100, &[]).await.unwrap_err();

    assert_eq!(error, EthereumBaseLayerError::RpcError(RpcError::ErrorResp(auth_error)));
    // No bisection was attempted: the queue is empty, so a follow-up single-block success is the
    // very next response (would panic if the previous call had popped extra half-range requests).
    asserter.push_success(&Vec::<Log>::new());
    base_layer.events(1..=1, &[]).await.unwrap();
}

#[tokio::test]
async fn is_range_too_large_error_classification() {
    let oversized_messages = [
        "query returned more than 10000 results",
        "Query returned more than 10000 results",
        "response size exceeded the limit",
        "query timeout exceeded",
        "the query took too long",
        "block range is too large",
        "range too large",
        "query exceed maximum block range",
        "block range limit exceeded",
    ];
    for message in oversized_messages {
        let error = EthereumBaseLayerError::RpcError(RpcError::ErrorResp(ErrorPayload {
            code: -32000,
            message: message.into(),
            data: None,
        }));
        assert!(is_range_too_large_error(&error), "expected oversized for message: {message}");
    }

    // HTTP 413 and matching body -> true.
    assert!(is_range_too_large_error(&http_error(413, "payload too large")));
    assert!(is_range_too_large_error(&http_error(200, "response size exceeded")));

    // Timeout -> true.
    assert!(is_range_too_large_error(&EthereumBaseLayerError::ProviderTimeout(
        make_elapsed().await
    )));

    // Auth / rate-limit / server / connection errors -> false.
    for status in [401, 403, 429, 500, 503] {
        assert!(
            !is_range_too_large_error(&http_error(status, "")),
            "expected non-oversized for status: {status}"
        );
    }
    let not_oversized_messages =
        ["unauthorized", "limit exceeded", "rate limited", "invalid params"];
    for message in not_oversized_messages {
        let error = EthereumBaseLayerError::RpcError(RpcError::ErrorResp(ErrorPayload {
            code: -32000,
            message: message.into(),
            data: None,
        }));
        assert!(!is_range_too_large_error(&error), "expected non-oversized for message: {message}");
    }
    assert!(!is_range_too_large_error(&EthereumBaseLayerError::RpcError(RpcError::Transport(
        TransportErrorKind::BackendGone
    ))));
    assert!(!is_range_too_large_error(&EthereumBaseLayerError::RpcError(
        TransportErrorKind::custom_str("connection refused")
    )));
    assert!(!is_range_too_large_error(&EthereumBaseLayerError::CalldataValueOutOfRange(
        alloy::primitives::U256::from(1_u8)
    )));
}

fn http_error(status: u16, body: &str) -> EthereumBaseLayerError {
    EthereumBaseLayerError::RpcError(RpcError::Transport(TransportErrorKind::HttpError(
        HttpError { status, body: body.to_string() },
    )))
}
