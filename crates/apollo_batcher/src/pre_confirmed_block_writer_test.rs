use std::sync::Arc;
use std::time::Duration;

use apollo_batcher_types::batcher_types::Round;
use apollo_starknet_client::reader::StateDiff;
use reqwest::StatusCode;
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

use crate::cende_client_types::{CendeBlockMetadata, StarknetClientTransactionReceipt};
use crate::pre_confirmed_block_writer::{
    PreconfirmedBlockWriter,
    PreconfirmedBlockWriterInput,
    PreconfirmedBlockWriterTrait,
    PreconfirmedTxSender,
};
use crate::pre_confirmed_cende_client::{
    MockPreconfirmedCendeClientTrait,
    PreconfirmedCendeClientError,
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
) -> (PreconfirmedBlockWriter, PreconfirmedTxSender) {
    let (pre_confirmed_tx_sender, pre_confirmed_tx_receiver) =
        tokio::sync::mpsc::channel(CHANNEL_SIZE);
    let writer = PreconfirmedBlockWriter::new(
        PreconfirmedBlockWriterInput {
            block_number: TEST_BLOCK_NUMBER,
            round: TEST_ROUND,
            block_metadata: test_block_metadata(),
        },
        pre_confirmed_tx_receiver,
        Arc::new(mock_client),
        write_interval_ms,
    );
    (writer, pre_confirmed_tx_sender)
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

    let (mut writer, pre_confirmed_tx_sender) = create_writer(mock_client, WRITE_INTERVAL_MS);

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

    let (mut writer, pre_confirmed_tx_sender) = create_writer(mock_client, WRITE_INTERVAL_MS);

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
    let (mut writer, pre_confirmed_tx_sender) = create_writer(mock_client, LONG_WRITE_INTERVAL_MS);

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

/// The writer completes gracefully when the cende client returns errors (writes are best-effort).
/// Uses a sequence of Ok -> Err -> Ok to verify the writer continues after a mid-stream failure.
#[tokio::test]
async fn write_error_is_ignored() {
    // Synchronization primitives to coordinate test phases with mock calls.
    let first_write_done = Arc::new(Notify::new());
    let first_write_done_clone = first_write_done.clone();
    let second_write_done = Arc::new(Notify::new());
    let second_write_done_clone = second_write_done.clone();
    let third_write_done = Arc::new(Notify::new());
    let third_write_done_clone = third_write_done.clone();

    let mut seq = mockall::Sequence::new();
    let mut mock_client = MockPreconfirmedCendeClientTrait::new();
    mock_client.expect_write_pre_confirmed_block().times(1).in_sequence(&mut seq).returning(
        move |_| {
            first_write_done_clone.notify_one();
            Ok(())
        },
    );
    mock_client.expect_write_pre_confirmed_block().times(1).in_sequence(&mut seq).returning(
        move |_| {
            second_write_done_clone.notify_one();
            Err(PreconfirmedCendeClientError::RequestFailed(StatusCode::INTERNAL_SERVER_ERROR))
        },
    );
    mock_client.expect_write_pre_confirmed_block().times(1..).returning(move |_| {
        third_write_done_clone.notify_one();
        Ok(())
    });

    let (mut writer, pre_confirmed_tx_sender) = create_writer(mock_client, WRITE_INTERVAL_MS);

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
