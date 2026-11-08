use std::sync::{Arc, Mutex};
use std::time::Duration;

use apollo_batcher_types::batcher_types::Round;
use apollo_starknet_client::reader::StateDiff;
use async_trait::async_trait;
use reqwest::StatusCode;
use rstest::rstest;
use starknet_api::block::{
    BlockNumber,
    BlockTimestamp,
    GasPrice,
    GasPricePerToken,
    StarknetVersion,
};
use starknet_api::core::ContractAddress;
use starknet_api::data_availability::L1DataAvailabilityMode;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::cende_client_types::{CendeBlockMetadata, StarknetClientTransactionReceipt};
use crate::pre_confirmed_block_writer::{
    retry_backoff,
    PreconfirmedBlockWriter,
    PreconfirmedBlockWriterInput,
    PreconfirmedBlockWriterTrait,
    PreconfirmedTxSender,
    RetryState,
    WriteOutcome,
    MAX_FINAL_WRITE_ATTEMPTS,
};
use crate::pre_confirmed_cende_client::{
    CendeWritePreconfirmedBlock,
    MockPreconfirmedCendeClientTrait,
    PreconfirmedCendeClientError,
    PreconfirmedCendeClientResult,
    PreconfirmedCendeClientTrait,
};
use crate::test_utils::test_txs;

const TEST_BLOCK_NUMBER: BlockNumber = BlockNumber(1);
const TEST_ROUND: Round = 0;
const CHANNEL_SIZE: usize = 100;
const WRITE_INTERVAL_MS: u64 = 10;
// Large enough that the timer's second tick never fires during the test; the first tick of
// `tokio::time::interval` is always immediately ready, so the initial empty-block write still
// happens.
const LONG_WRITE_INTERVAL_MS: u64 = 60_000;

fn test_block_metadata() -> CendeBlockMetadata {
    CendeBlockMetadata {
        status: "PENDING",
        starknet_version: StarknetVersion::default(),
        l1_da_mode: L1DataAvailabilityMode::Calldata,
        l1_gas_price: GasPricePerToken { price_in_fri: GasPrice(1), price_in_wei: GasPrice(1) },
        l1_data_gas_price: GasPricePerToken {
            price_in_fri: GasPrice(1),
            price_in_wei: GasPrice(1),
        },
        l2_gas_price: GasPricePerToken { price_in_fri: GasPrice(1), price_in_wei: GasPrice(1) },
        timestamp: BlockTimestamp(0),
        sequencer_address: ContractAddress::default(),
    }
}

fn create_writer(
    mock_client: MockPreconfirmedCendeClientTrait,
    write_interval_ms: u64,
) -> (PreconfirmedBlockWriter, PreconfirmedTxSender, CancellationToken) {
    let (pre_confirmed_tx_sender, pre_confirmed_tx_receiver) =
        tokio::sync::mpsc::channel(CHANNEL_SIZE);
    let cancellation_token = CancellationToken::new();
    let writer = PreconfirmedBlockWriter::new(
        PreconfirmedBlockWriterInput {
            block_number: TEST_BLOCK_NUMBER,
            round: TEST_ROUND,
            block_metadata: test_block_metadata(),
        },
        pre_confirmed_tx_receiver,
        Arc::new(mock_client),
        write_interval_ms,
        cancellation_token.clone(),
    );
    (writer, pre_confirmed_tx_sender, cancellation_token)
}

/// Closing the channel immediately triggers a single write with an empty transaction list
/// (the initial `pending_changes = true` guarantees at least one write).
#[tokio::test]
async fn write_empty_block_on_channel_close() {
    let mut mock_client = MockPreconfirmedCendeClientTrait::new();
    mock_client
        .expect_write_pre_confirmed_block()
        .times(1)
        .withf(|block| {
            block.block_number == TEST_BLOCK_NUMBER
                && block.round == TEST_ROUND
                && block.write_iteration == 0
                && block.pre_confirmed_block.transactions.is_empty()
        })
        .returning(|_| Ok(()));

    let (mut writer, pre_confirmed_tx_sender, _cancellation_token) =
        create_writer(mock_client, WRITE_INTERVAL_MS);

    drop(pre_confirmed_tx_sender);

    writer.run().await;
}

/// Pre-confirmed transactions appear in the written block with receipts and state diffs.
#[tokio::test]
async fn write_pre_confirmed_transactions() {
    let txs = test_txs(0..2);
    let expected_hashes: Vec<_> = txs.iter().map(|tx| tx.tx_hash()).collect();

    let mut mock_client = MockPreconfirmedCendeClientTrait::new();
    mock_client
        .expect_write_pre_confirmed_block()
        .withf(move |block| {
            let hashes: Vec<_> = block
                .pre_confirmed_block
                .transactions
                .iter()
                .map(|t| t.transaction_hash())
                .collect();
            hashes == expected_hashes
        })
        .times(1)
        .returning(|_| Ok(()));
    mock_client.expect_write_pre_confirmed_block().returning(|_| Ok(()));

    let (mut writer, pre_confirmed_tx_sender, _cancellation_token) =
        create_writer(mock_client, WRITE_INTERVAL_MS);

    let writer_handle = tokio::spawn(async move { writer.run().await });

    for tx in txs {
        pre_confirmed_tx_sender
            .send((tx, StarknetClientTransactionReceipt::default(), StateDiff::default()))
            .await
            .unwrap();
    }

    tokio::time::sleep(Duration::from_millis(WRITE_INTERVAL_MS)).await;

    drop(pre_confirmed_tx_sender);

    writer_handle.await.unwrap();
}

/// Transactions sent after the timer has already fired are flushed via the final write path
/// (after the select loop breaks on channel close).
#[tokio::test]
async fn final_write_includes_pending_changes() {
    let txs = test_txs(0..2);
    let expected_hashes: Vec<_> = txs.iter().map(|tx| tx.tx_hash()).collect();

    // Signalled by the catch-all mock once the initial empty-block write completes, so we don't
    // race against the interval timer when deciding when to send transactions.
    let initial_write_done = Arc::new(Notify::new());
    let initial_write_done_clone = initial_write_done.clone();

    let mut mock_client = MockPreconfirmedCendeClientTrait::new();
    mock_client
        .expect_write_pre_confirmed_block()
        .withf(move |block| {
            let hashes: Vec<_> = block
                .pre_confirmed_block
                .transactions
                .iter()
                .map(|t| t.transaction_hash())
                .collect();
            hashes == expected_hashes
        })
        .times(1)
        .returning(|_| Ok(()));
    mock_client.expect_write_pre_confirmed_block().times(1).returning(move |_| {
        initial_write_done_clone.notify_one();
        Ok(())
    });

    // Use a large interval so the timer's second tick never fires during this test. The first tick
    // is always immediately ready, so the initial empty-block write still happens; subsequent ticks
    // are 60 s away, eliminating the select! race between the timer arm and the recv arm.
    let (mut writer, pre_confirmed_tx_sender, _cancellation_token) =
        create_writer(mock_client, LONG_WRITE_INTERVAL_MS);

    let writer_handle = tokio::spawn(async move { writer.run().await });

    // Wait deterministically for the initial empty-block write to complete.
    initial_write_done.notified().await;

    for tx in txs {
        pre_confirmed_tx_sender
            .send((tx, StarknetClientTransactionReceipt::default(), StateDiff::default()))
            .await
            .unwrap();
    }

    drop(pre_confirmed_tx_sender);

    writer_handle.await.unwrap();
}

/// The writer keeps writing after a mid-stream server error.
/// Uses a sequence of Ok -> Err -> Ok to verify the writer continues after the failure.
#[tokio::test]
async fn writer_recovers_after_server_error() {
    // Synchronization primitives to coordinate test phases with mock calls.
    let first_write_done = Arc::new(Notify::new());
    let first_write_done_clone = first_write_done.clone();
    let second_write_done = Arc::new(Notify::new());
    let second_write_done_clone = second_write_done.clone();
    let third_write_done = Arc::new(Notify::new());
    let third_write_done_clone = third_write_done.clone();

    let mut sequence = mockall::Sequence::new();
    let mut mock_client = MockPreconfirmedCendeClientTrait::new();
    mock_client.expect_write_pre_confirmed_block().times(1).in_sequence(&mut sequence).returning(
        move |_| {
            first_write_done_clone.notify_one();
            Ok(())
        },
    );
    mock_client.expect_write_pre_confirmed_block().times(1).in_sequence(&mut sequence).returning(
        move |_| {
            second_write_done_clone.notify_one();
            Err(PreconfirmedCendeClientError::RequestFailed(StatusCode::INTERNAL_SERVER_ERROR))
        },
    );
    mock_client.expect_write_pre_confirmed_block().times(1..).returning(move |_| {
        third_write_done_clone.notify_one();
        Ok(())
    });

    let (mut writer, pre_confirmed_tx_sender, _cancellation_token) =
        create_writer(mock_client, WRITE_INTERVAL_MS);

    // Send txs before spawning so the initial write includes transactions.
    for tx in test_txs(0..3) {
        pre_confirmed_tx_sender
            .send((tx, StarknetClientTransactionReceipt::default(), StateDiff::default()))
            .await
            .unwrap();
    }

    let writer_handle = tokio::spawn(async move { writer.run().await });

    // Wait for the initial write (Ok) to complete.
    first_write_done.notified().await;

    // Trigger a second write (Err).
    for tx in test_txs(3..4) {
        pre_confirmed_tx_sender
            .send((tx, StarknetClientTransactionReceipt::default(), StateDiff::default()))
            .await
            .unwrap();
    }
    second_write_done.notified().await;

    // Trigger a third write (Ok) to confirm the writer recovered.
    for tx in test_txs(4..5) {
        pre_confirmed_tx_sender
            .send((tx, StarknetClientTransactionReceipt::default(), StateDiff::default()))
            .await
            .unwrap();
    }
    third_write_done.notified().await;

    drop(pre_confirmed_tx_sender);

    writer_handle.await.unwrap();
}

/// A write that fails with a retryable error is resent by the final write, with a new iteration,
/// even when no new transactions arrived after it.
#[tokio::test]
async fn failed_write_is_resent() {
    let initial_write_done = Arc::new(Notify::new());
    let initial_write_done_clone = initial_write_done.clone();

    let mut sequence = mockall::Sequence::new();
    let mut mock_client = MockPreconfirmedCendeClientTrait::new();
    mock_client
        .expect_write_pre_confirmed_block()
        .times(1)
        .in_sequence(&mut sequence)
        .withf(|block| block.write_iteration == 0)
        .returning(move |_| {
            initial_write_done_clone.notify_one();
            Err(PreconfirmedCendeClientError::RequestFailed(StatusCode::INTERNAL_SERVER_ERROR))
        });
    mock_client
        .expect_write_pre_confirmed_block()
        .times(1)
        .in_sequence(&mut sequence)
        .withf(|block| block.write_iteration == 1)
        .returning(|_| Ok(()));

    let (mut writer, pre_confirmed_tx_sender, _cancellation_token) =
        create_writer(mock_client, LONG_WRITE_INTERVAL_MS);

    let writer_handle = tokio::spawn(async move { writer.run().await });

    initial_write_done.notified().await;
    drop(pre_confirmed_tx_sender);

    writer_handle.await.unwrap();
}

/// A write rejected by the recorder (4xx) is not resent.
#[tokio::test]
async fn rejected_write_is_not_retried() {
    let rejected_write_done = Arc::new(Notify::new());
    let rejected_write_done_clone = rejected_write_done.clone();

    let mut mock_client = MockPreconfirmedCendeClientTrait::new();
    mock_client.expect_write_pre_confirmed_block().times(1).returning(move |_| {
        rejected_write_done_clone.notify_one();
        Err(PreconfirmedCendeClientError::RequestFailed(StatusCode::BAD_REQUEST))
    });

    let (mut writer, pre_confirmed_tx_sender, _cancellation_token) =
        create_writer(mock_client, WRITE_INTERVAL_MS);

    let writer_handle = tokio::spawn(async move { writer.run().await });

    rejected_write_done.notified().await;
    // Gives the timer several ticks in which a wrongful retry would show up.
    tokio::time::sleep(Duration::from_millis(5 * WRITE_INTERVAL_MS)).await;
    drop(pre_confirmed_tx_sender);

    writer_handle.await.unwrap();
}

/// A final write that fails with a retryable error is resent with a new iteration.
#[tokio::test]
async fn final_write_is_retried() {
    let initial_write_done = Arc::new(Notify::new());
    let initial_write_done_clone = initial_write_done.clone();

    let mut sequence = mockall::Sequence::new();
    let mut mock_client = MockPreconfirmedCendeClientTrait::new();
    mock_client
        .expect_write_pre_confirmed_block()
        .times(1)
        .in_sequence(&mut sequence)
        .withf(|block| block.write_iteration == 0)
        .returning(move |_| {
            initial_write_done_clone.notify_one();
            Ok(())
        });
    mock_client
        .expect_write_pre_confirmed_block()
        .times(1)
        .in_sequence(&mut sequence)
        .withf(|block| {
            block.write_iteration == 1 && block.pre_confirmed_block.transactions.len() == 1
        })
        .returning(|_| {
            Err(PreconfirmedCendeClientError::RequestFailed(StatusCode::INTERNAL_SERVER_ERROR))
        });
    mock_client
        .expect_write_pre_confirmed_block()
        .times(1)
        .in_sequence(&mut sequence)
        .withf(|block| {
            block.write_iteration == 2 && block.pre_confirmed_block.transactions.len() == 1
        })
        .returning(|_| Ok(()));

    let (mut writer, pre_confirmed_tx_sender, _cancellation_token) =
        create_writer(mock_client, LONG_WRITE_INTERVAL_MS);

    let writer_handle = tokio::spawn(async move { writer.run().await });

    initial_write_done.notified().await;
    for tx in test_txs(0..1) {
        pre_confirmed_tx_sender
            .send((tx, StarknetClientTransactionReceipt::default(), StateDiff::default()))
            .await
            .unwrap();
    }
    drop(pre_confirmed_tx_sender);

    writer_handle.await.unwrap();
}

/// Never answers its first write, and records the iterations of the writes that complete.
struct StuckFirstWriteClient {
    first_write_started: Arc<Notify>,
    completed_write_iterations: Mutex<Vec<u64>>,
}

#[async_trait]
impl PreconfirmedCendeClientTrait for StuckFirstWriteClient {
    async fn write_pre_confirmed_block(
        &self,
        pre_confirmed_block: CendeWritePreconfirmedBlock,
    ) -> PreconfirmedCendeClientResult<()> {
        if pre_confirmed_block.write_iteration == 0 {
            self.first_write_started.notify_one();
            std::future::pending::<()>().await;
        }
        self.completed_write_iterations.lock().unwrap().push(pre_confirmed_block.write_iteration);
        Ok(())
    }
}

/// A write stuck in flight when the block closes does not hold back the final write.
#[tokio::test]
async fn stuck_write_does_not_block_final_write() {
    let first_write_started = Arc::new(Notify::new());
    let client = Arc::new(StuckFirstWriteClient {
        first_write_started: first_write_started.clone(),
        completed_write_iterations: Mutex::new(Vec::new()),
    });
    let (pre_confirmed_tx_sender, pre_confirmed_tx_receiver) =
        tokio::sync::mpsc::channel(CHANNEL_SIZE);
    let mut writer = PreconfirmedBlockWriter::new(
        PreconfirmedBlockWriterInput {
            block_number: TEST_BLOCK_NUMBER,
            round: TEST_ROUND,
            block_metadata: test_block_metadata(),
        },
        pre_confirmed_tx_receiver,
        client.clone(),
        LONG_WRITE_INTERVAL_MS,
        CancellationToken::new(),
    );

    let writer_handle = tokio::spawn(async move { writer.run().await });

    first_write_started.notified().await;
    drop(pre_confirmed_tx_sender);

    tokio::time::timeout(Duration::from_secs(10), writer_handle)
        .await
        .expect("The writer should not wait for the stuck write.")
        .unwrap();
    assert_eq!(*client.completed_write_iterations.lock().unwrap(), vec![1]);
}

/// The final write gives up after `MAX_FINAL_WRITE_ATTEMPTS` retryable failures.
#[tokio::test]
async fn final_write_gives_up_after_max_attempts() {
    let initial_write_done = Arc::new(Notify::new());
    let initial_write_done_clone = initial_write_done.clone();

    let mut sequence = mockall::Sequence::new();
    let mut mock_client = MockPreconfirmedCendeClientTrait::new();
    mock_client
        .expect_write_pre_confirmed_block()
        .times(1)
        .in_sequence(&mut sequence)
        .withf(|block| block.write_iteration == 0)
        .returning(move |_| {
            initial_write_done_clone.notify_one();
            Ok(())
        });
    for expected_iteration in 1..=u64::from(MAX_FINAL_WRITE_ATTEMPTS) {
        mock_client
            .expect_write_pre_confirmed_block()
            .times(1)
            .in_sequence(&mut sequence)
            .withf(move |block| block.write_iteration == expected_iteration)
            .returning(|_| {
                Err(PreconfirmedCendeClientError::RequestFailed(StatusCode::INTERNAL_SERVER_ERROR))
            });
    }

    let (mut writer, pre_confirmed_tx_sender, _cancellation_token) =
        create_writer(mock_client, LONG_WRITE_INTERVAL_MS);

    let writer_handle = tokio::spawn(async move { writer.run().await });

    initial_write_done.notified().await;
    for tx in test_txs(0..1) {
        pre_confirmed_tx_sender
            .send((tx, StarknetClientTransactionReceipt::default(), StateDiff::default()))
            .await
            .unwrap();
    }
    drop(pre_confirmed_tx_sender);

    tokio::time::timeout(Duration::from_secs(10), writer_handle)
        .await
        .expect("The writer should exit after the last attempt.")
        .unwrap();
}

#[rstest]
#[case::first_failure(1, 50)]
#[case::second_failure(2, 100)]
#[case::fifth_failure(5, 800)]
#[case::capped(6, 1_000)]
#[case::no_overflow(u32::MAX, 1_000)]
fn retry_backoff_doubles_up_to_the_cap(
    #[case] consecutive_failures: u32,
    #[case] expected_millis: u64,
) {
    assert_eq!(retry_backoff(consecutive_failures), Duration::from_millis(expected_millis));
}

/// Asserts that writes stay blocked until exactly `backoff` has elapsed on the paused clock.
async fn assert_write_allowed_after(retry_state: &RetryState, backoff: Duration) {
    assert!(!retry_state.write_allowed());
    tokio::time::advance(backoff - Duration::from_millis(1)).await;
    assert!(!retry_state.write_allowed());
    tokio::time::advance(Duration::from_millis(1)).await;
    assert!(retry_state.write_allowed());
}

/// The backoff restarts from the initial value after a successful write.
#[tokio::test(start_paused = true)]
async fn backoff_resets_after_success() {
    let mut retry_state = RetryState::default();

    assert!(retry_state.should_resend_after(WriteOutcome::Failed));
    assert_write_allowed_after(&retry_state, Duration::from_millis(50)).await;
    assert!(retry_state.should_resend_after(WriteOutcome::Failed));
    assert_write_allowed_after(&retry_state, Duration::from_millis(100)).await;

    assert!(!retry_state.should_resend_after(WriteOutcome::Written));
    assert!(retry_state.write_allowed());

    assert!(retry_state.should_resend_after(WriteOutcome::Failed));
    assert_write_allowed_after(&retry_state, Duration::from_millis(50)).await;
}

/// Cancelling the writer ends it without the final write: an aborted proposal's buffered
/// transactions never reach the feed.
#[tokio::test]
async fn cancelled_writer_skips_final_write() {
    let initial_write_done = Arc::new(Notify::new());
    let mut mock_client = MockPreconfirmedCendeClientTrait::new();
    // The first tick writes the empty initial block; nothing else may be written.
    let notify_initial_write = initial_write_done.clone();
    mock_client
        .expect_write_pre_confirmed_block()
        .times(1)
        .withf(|block| block.pre_confirmed_block.transactions.is_empty())
        .returning(move |_| {
            notify_initial_write.notify_one();
            Ok(())
        });

    let (mut writer, pre_confirmed_tx_sender, cancellation_token) =
        create_writer(mock_client, LONG_WRITE_INTERVAL_MS);
    let writer_handle = tokio::spawn(async move { writer.run().await });
    initial_write_done.notified().await;

    let tx = test_txs(0..1).pop().unwrap();
    pre_confirmed_tx_sender
        .send((tx, StarknetClientTransactionReceipt::default(), StateDiff::default()))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(WRITE_INTERVAL_MS)).await;

    cancellation_token.cancel();
    // The sender stays open: the writer must stop on cancellation alone.
    tokio::time::timeout(Duration::from_secs(5), writer_handle)
        .await
        .expect("writer did not stop on cancellation")
        .unwrap();
    drop(pre_confirmed_tx_sender);
}
