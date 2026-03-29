use std::sync::Arc;
use std::time::Duration;

use apollo_batcher_types::batcher_types::Round;
use apollo_starknet_client::reader::StateDiff;
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

use crate::cende_client_types::{CendeBlockMetadata, StarknetClientTransactionReceipt};
use crate::pre_confirmed_block_writer::{
    CandidateTxSender,
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
) -> (PreconfirmedBlockWriter, CandidateTxSender, PreconfirmedTxSender) {
    let (candidate_tx_sender, candidate_tx_receiver) = tokio::sync::mpsc::channel(CHANNEL_SIZE);
    let (pre_confirmed_tx_sender, pre_confirmed_tx_receiver) =
        tokio::sync::mpsc::channel(CHANNEL_SIZE);
    let writer = PreconfirmedBlockWriter::new(
        PreconfirmedBlockWriterInput {
            block_number: TEST_BLOCK_NUMBER,
            round: TEST_ROUND,
            block_metadata: test_block_metadata(),
        },
        candidate_tx_receiver,
        pre_confirmed_tx_receiver,
        Arc::new(mock_client),
        WRITE_INTERVAL_MS,
    );
    (writer, candidate_tx_sender, pre_confirmed_tx_sender)
}

/// Drops one or both channels depending on `$case`, leaving the other alive in the caller's scope.
/// A macro is required because a function would take ownership of both and drop both on return.
/// - case 0: drop candidate channel only
/// - case 1: drop pre-confirmed channel only
/// - case 2: drop both channels
macro_rules! drop_channels {
    ($candidate:expr, $pre_confirmed:expr, $case:expr) => {
        match $case {
            0 => drop($candidate),
            1 => drop($pre_confirmed),
            _ => {
                drop($candidate);
                drop($pre_confirmed);
            }
        }
    };
}

/// Closing either channel immediately triggers a single write with an empty transaction list
/// (the initial `pending_changes = true` guarantees at least one write).
#[rstest]
#[case::drop_candidate_first(0)]
#[case::drop_pre_confirmed_first(1)]
#[case::drop_both(2)]
#[tokio::test]
async fn write_empty_block_on_channel_close(#[case] drop_case: u8) {
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

    let (mut writer, candidate_tx_sender, pre_confirmed_tx_sender) = create_writer(mock_client);

    drop_channels!(candidate_tx_sender, pre_confirmed_tx_sender, drop_case);

    writer.run().await;
}

/// Candidate transactions appear in the written block with `None` receipts and state diffs.
#[tokio::test]
async fn write_candidate_transactions() {
    let txs = test_txs(0..3);
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
                && block.pre_confirmed_block.transaction_receipts.iter().all(|r| r.is_none())
                && block.pre_confirmed_block.transaction_state_diffs.iter().all(|sd| sd.is_none())
        })
        .times(1)
        .returning(|_| Ok(()));
    mock_client.expect_write_pre_confirmed_block().returning(|_| Ok(()));

    let (mut writer, candidate_tx_sender, pre_confirmed_tx_sender) = create_writer(mock_client);

    let writer_handle = tokio::spawn(async move { writer.run().await });

    candidate_tx_sender.send(txs).await.unwrap();
    tokio::time::sleep(Duration::from_millis(WRITE_INTERVAL_MS)).await;

    // Need to close one of the channels in order to make the write_handle task finish.
    drop(candidate_tx_sender);
    drop(pre_confirmed_tx_sender);

    writer_handle.await.unwrap();
}

/// Pre-confirmed transactions appear in the written block with `Some` receipts and state diffs.
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
                && block.pre_confirmed_block.transaction_receipts.iter().all(|r| r.is_some())
                && block.pre_confirmed_block.transaction_state_diffs.iter().all(|sd| sd.is_some())
        })
        .times(1)
        .returning(|_| Ok(()));
    mock_client.expect_write_pre_confirmed_block().returning(|_| Ok(()));

    let (mut writer, candidate_tx_sender, pre_confirmed_tx_sender) = create_writer(mock_client);

    let writer_handle = tokio::spawn(async move { writer.run().await });

    for tx in txs {
        pre_confirmed_tx_sender
            .send((tx, StarknetClientTransactionReceipt::default(), StateDiff::default()))
            .await
            .unwrap();
    }

    tokio::time::sleep(Duration::from_millis(WRITE_INTERVAL_MS)).await;

    drop(candidate_tx_sender);
    drop(pre_confirmed_tx_sender);

    writer_handle.await.unwrap();
}

/// When a transaction is already present as pre-confirmed (with receipt), a later candidate with
/// the same hash does not overwrite it.
#[tokio::test]
async fn candidate_does_not_overwrite_pre_confirmed() {
    let tx = test_txs(0..1).into_iter().next().unwrap();
    let expected_hash = tx.tx_hash();

    let mut mock_client = MockPreconfirmedCendeClientTrait::new();
    mock_client
        .expect_write_pre_confirmed_block()
        .withf(move |block| {
            block
                .pre_confirmed_block
                .transactions
                .iter()
                .any(|t| t.transaction_hash() == expected_hash)
                && block.pre_confirmed_block.transaction_receipts.iter().all(|r| r.is_some())
                && block.pre_confirmed_block.transaction_state_diffs.iter().all(|sd| sd.is_some())
        })
        .times(1)
        .returning(|_| Ok(()));
    mock_client.expect_write_pre_confirmed_block().returning(|_| Ok(()));

    let (mut writer, candidate_tx_sender, pre_confirmed_tx_sender) = create_writer(mock_client);

    let writer_handle = tokio::spawn(async move { writer.run().await });

    pre_confirmed_tx_sender
        .send((tx.clone(), StarknetClientTransactionReceipt::default(), StateDiff::default()))
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(WRITE_INTERVAL_MS)).await;

    candidate_tx_sender.send(vec![tx]).await.unwrap();

    tokio::time::sleep(Duration::from_millis(WRITE_INTERVAL_MS)).await;

    drop(candidate_tx_sender);
    drop(pre_confirmed_tx_sender);

    writer_handle.await.unwrap();
}

/// Transactions sent after the timer has already fired are flushed via the final write path
/// (after the select loop breaks on channel close).
#[tokio::test]
async fn final_write_includes_pending_changes() {
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
    mock_client.expect_write_pre_confirmed_block().times(1).returning(|_| Ok(()));

    let (mut writer, candidate_tx_sender, pre_confirmed_tx_sender) = create_writer(mock_client);

    let writer_handle = tokio::spawn(async move { writer.run().await });

    // Wait for the initial empty-block write to complete.
    tokio::time::sleep(Duration::from_millis(WRITE_INTERVAL_MS)).await;

    candidate_tx_sender.send(txs).await.unwrap();

    // Give the writer enough time to process the channel message but close channels before the
    // next timer-triggered write.
    tokio::task::yield_now().await;

    drop(candidate_tx_sender);
    drop(pre_confirmed_tx_sender);

    writer_handle.await.unwrap();
}

/// The writer completes gracefully when the cende client returns errors (writes are best-effort).
/// Uses a sequence of Ok -> Err -> Ok to verify the writer continues after a mid-stream failure.
#[tokio::test]
async fn write_error_is_ignored() {
    let mut seq = mockall::Sequence::new();
    let mut mock_client = MockPreconfirmedCendeClientTrait::new();
    mock_client
        .expect_write_pre_confirmed_block()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_| Ok(()));
    mock_client.expect_write_pre_confirmed_block().times(1).in_sequence(&mut seq).returning(|_| {
        Err(PreconfirmedCendeClientError::RequestFailed(StatusCode::INTERNAL_SERVER_ERROR))
    });
    mock_client.expect_write_pre_confirmed_block().times(1..).returning(|_| Ok(()));

    let (mut writer, candidate_tx_sender, pre_confirmed_tx_sender) = create_writer(mock_client);

    // Send txs before spawning so the initial write includes transactions.
    candidate_tx_sender.send(test_txs(0..3)).await.unwrap();

    let writer_handle = tokio::spawn(async move { writer.run().await });

    // Let the initial write (Ok) complete.
    tokio::time::sleep(Duration::from_millis(WRITE_INTERVAL_MS)).await;

    // Trigger a second write (Err).
    candidate_tx_sender.send(test_txs(3..4)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(WRITE_INTERVAL_MS)).await;

    // Trigger a third write (Ok) to confirm the writer recovered.
    candidate_tx_sender.send(test_txs(4..5)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(WRITE_INTERVAL_MS)).await;

    drop(candidate_tx_sender);
    drop(pre_confirmed_tx_sender);

    writer_handle.await.unwrap();
}
