use assert_matches::assert_matches;
use blockifier::blockifier::transaction_executor::{
    TransactionExecutorError,
    TransactionExecutorError as BlockifierTransactionExecutorError,
};
use blockifier::bouncer::BouncerWeights;
use blockifier::transaction::objects::TransactionExecutionInfo;
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use indexmap::IndexMap;
use mockall::predicate::eq;
use mockall::Sequence;
use rstest::rstest;
use starknet_api::executable_transaction::Transaction;
use starknet_api::felt;
use starknet_api::transaction::TransactionHash;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::block_builder::{
    BlockBuilder,
    BlockBuilderError,
    BlockBuilderResult,
    BlockBuilderTrait,
    BlockExecutionArtifacts,
};
use crate::test_utils::test_txs;
use crate::transaction_executor::MockTransactionExecutorTrait;
use crate::transaction_provider::{MockTransactionProvider, NextTxs};

const BLOCK_GENERATION_DEADLINE_SECS: u64 = 1;
const TX_CHANNEL_SIZE: usize = 50;
const TX_CHUNK_SIZE: usize = 3;

fn output_channel() -> (UnboundedSender<Transaction>, UnboundedReceiver<Transaction>) {
    tokio::sync::mpsc::unbounded_channel()
}

fn block_execution_artifacts(
    execution_infos: IndexMap<TransactionHash, TransactionExecutionInfo>,
) -> BlockExecutionArtifacts {
    BlockExecutionArtifacts {
        execution_infos,
        commitment_state_diff: Default::default(),
        visited_segments_mapping: Default::default(),
        bouncer_weights: BouncerWeights { gas: 100, ..BouncerWeights::empty() },
    }
}

// Filling the execution_info with some non-default values to make sure the block_builder uses them.
fn execution_info() -> TransactionExecutionInfo {
    TransactionExecutionInfo { revert_error: Some("Test string".to_string()), ..Default::default() }
}

fn one_chunk_test_expectations(
    input_txs: &[Transaction],
) -> (MockTransactionExecutorTrait, MockTransactionProvider, BlockExecutionArtifacts) {
    let block_size = input_txs.len();
    let (mock_transaction_executor, expected_block_artifacts) =
        one_chunk_mock_executor(input_txs, block_size);

    let mock_tx_provider = mock_tx_provider_limitless_calls(1, vec![input_txs.to_vec()]);

    (mock_transaction_executor, mock_tx_provider, expected_block_artifacts)
}

fn one_chunk_mock_executor(
    input_txs: &[Transaction],
    block_size: usize,
) -> (MockTransactionExecutorTrait, BlockExecutionArtifacts) {
    let input_txs_cloned = input_txs.to_vec();
    let mut mock_transaction_executor = MockTransactionExecutorTrait::new();

    mock_transaction_executor
        .expect_add_txs_to_block()
        .withf(move |blockifier_input| compare_tx_hashes(&input_txs_cloned, blockifier_input))
        .return_once(move |_| (0..block_size).map(|_| Ok(execution_info())).collect());

    let expected_block_artifacts =
        set_close_block_expectations(&mut mock_transaction_executor, block_size);
    (mock_transaction_executor, expected_block_artifacts)
}

fn two_chunks_test_expectations(
    input_txs: &[Transaction],
) -> (MockTransactionExecutorTrait, MockTransactionProvider, BlockExecutionArtifacts) {
    let first_chunk = input_txs[..TX_CHUNK_SIZE].to_vec();
    let second_chunk = input_txs[TX_CHUNK_SIZE..].to_vec();
    let block_size = input_txs.len();

    let mut mock_transaction_executor = MockTransactionExecutorTrait::new();
    let mut mock_add_txs_to_block = |tx_chunk: Vec<Transaction>, seq: &mut Sequence| {
        mock_transaction_executor
            .expect_add_txs_to_block()
            .times(1)
            .in_sequence(seq)
            .withf(move |blockifier_input| compare_tx_hashes(&tx_chunk, blockifier_input))
            .returning(move |_| (0..TX_CHUNK_SIZE).map(move |_| Ok(execution_info())).collect());
    };

    let mut seq = Sequence::new();
    mock_add_txs_to_block(first_chunk.clone(), &mut seq);
    mock_add_txs_to_block(second_chunk.clone(), &mut seq);

    let expected_block_artifacts =
        set_close_block_expectations(&mut mock_transaction_executor, block_size);

    let mock_tx_provider = mock_tx_provider_limitless_calls(2, vec![first_chunk, second_chunk]);

    (mock_transaction_executor, mock_tx_provider, expected_block_artifacts)
}

fn empty_block_test_expectations()
-> (MockTransactionExecutorTrait, MockTransactionProvider, BlockExecutionArtifacts) {
    let mut mock_transaction_executor = MockTransactionExecutorTrait::new();
    mock_transaction_executor.expect_add_txs_to_block().times(0);

    let expected_block_artifacts = set_close_block_expectations(&mut mock_transaction_executor, 0);

    let mock_tx_provider = mock_tx_provider_limitless_calls(1, vec![vec![]]);

    (mock_transaction_executor, mock_tx_provider, expected_block_artifacts)
}

fn block_full_test_expectations(
    input_txs: &[Transaction],
    block_size: usize,
) -> (MockTransactionExecutorTrait, MockTransactionProvider, BlockExecutionArtifacts) {
    let input_txs_cloned = input_txs.to_vec();
    let mut mock_transaction_executor = MockTransactionExecutorTrait::new();

    mock_transaction_executor
        .expect_add_txs_to_block()
        .withf(move |blockifier_input| compare_tx_hashes(&input_txs_cloned, blockifier_input))
        .return_once(move |_| vec![Ok(execution_info()), Err(TransactionExecutorError::BlockFull)]);

    let expected_block_artifacts =
        set_close_block_expectations(&mut mock_transaction_executor, block_size);

    let mock_tx_provider = mock_tx_provider_limited_calls(1, vec![input_txs.to_vec()]);

    (mock_transaction_executor, mock_tx_provider, expected_block_artifacts)
}

fn test_expectations_with_delay(
    input_txs: &[Transaction],
) -> (MockTransactionExecutorTrait, MockTransactionProvider, BlockExecutionArtifacts) {
    let first_chunk = input_txs[0..TX_CHUNK_SIZE].to_vec();
    let first_chunk_copy = first_chunk.clone();
    let mut mock_transaction_executor = MockTransactionExecutorTrait::new();

    mock_transaction_executor
        .expect_add_txs_to_block()
        .withf(move |blockifier_input| compare_tx_hashes(&first_chunk, blockifier_input))
        .return_once(move |_| {
            std::thread::sleep(std::time::Duration::from_secs(BLOCK_GENERATION_DEADLINE_SECS));
            (0..TX_CHUNK_SIZE).map(move |_| Ok(execution_info())).collect()
        });

    let expected_block_artifacts =
        set_close_block_expectations(&mut mock_transaction_executor, TX_CHUNK_SIZE);

    let mock_tx_provider = mock_tx_provider_limited_calls(1, vec![first_chunk_copy]);

    (mock_transaction_executor, mock_tx_provider, expected_block_artifacts)
}

fn stream_done_test_expectations(
    input_txs: &[Transaction],
) -> (MockTransactionExecutorTrait, MockTransactionProvider, BlockExecutionArtifacts) {
    let block_size = input_txs.len();
    let input_txs_cloned = input_txs.to_vec();
    let mut mock_transaction_executor = MockTransactionExecutorTrait::new();

    mock_transaction_executor
        .expect_add_txs_to_block()
        .withf(move |blockifier_input| compare_tx_hashes(&input_txs_cloned, blockifier_input))
        .return_once(move |_| (0..block_size).map(|_| Ok(execution_info())).collect());

    let expected_block_artifacts =
        set_close_block_expectations(&mut mock_transaction_executor, block_size);

    let mock_tx_provider = mock_tx_provider_stream_done(input_txs.to_vec());

    (mock_transaction_executor, mock_tx_provider, expected_block_artifacts)
}

// Fill the executor outputs with some non-default values to make sure the block_builder uses
// them.
fn block_builder_expected_output(execution_info_len: usize) -> BlockExecutionArtifacts {
    let execution_info_len_u8 = u8::try_from(execution_info_len).unwrap();
    let execution_infos_mapping =
        (0..execution_info_len_u8).map(|i| (TransactionHash(felt!(i)), execution_info())).collect();
    block_execution_artifacts(execution_infos_mapping)
}

fn set_close_block_expectations(
    mock_transaction_executor: &mut MockTransactionExecutorTrait,
    block_size: usize,
) -> BlockExecutionArtifacts {
    let output_block_artifacts = block_builder_expected_output(block_size);
    let output_block_artifacts_copy = output_block_artifacts.clone();
    mock_transaction_executor.expect_close_block().return_once(move || {
        Ok((
            output_block_artifacts.commitment_state_diff,
            output_block_artifacts.visited_segments_mapping,
            output_block_artifacts.bouncer_weights,
        ))
    });
    output_block_artifacts_copy
}

/// Create a mock tx provider that will return the input chunks for number of chunks queries.
/// This function assumes constant chunk size of TX_CHUNK_SIZE.
fn mock_tx_provider_limited_calls(
    n_calls: usize,
    mut input_chunks: Vec<Vec<Transaction>>,
) -> MockTransactionProvider {
    let mut mock_tx_provider = MockTransactionProvider::new();
    mock_tx_provider
        .expect_get_txs()
        .times(n_calls)
        .with(eq(TX_CHUNK_SIZE))
        .returning(move |_n_txs| Ok(NextTxs::Txs(input_chunks.remove(0))));
    mock_tx_provider
}

fn mock_tx_provider_stream_done(input_chunk: Vec<Transaction>) -> MockTransactionProvider {
    let mut mock_tx_provider = MockTransactionProvider::new();
    let mut seq = Sequence::new();
    mock_tx_provider
        .expect_get_txs()
        .times(1)
        .in_sequence(&mut seq)
        .with(eq(TX_CHUNK_SIZE))
        .returning(move |_n_txs| Ok(NextTxs::Txs(input_chunk.clone())));
    mock_tx_provider
        .expect_get_txs()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_n_txs| Ok(NextTxs::End));
    mock_tx_provider
}

/// Create a mock tx provider client that will return the input chunks and then empty chunks.
/// This function assumes constant chunk size of TX_CHUNK_SIZE.
fn mock_tx_provider_limitless_calls(
    n_calls: usize,
    input_chunks: Vec<Vec<Transaction>>,
) -> MockTransactionProvider {
    let mut mock_tx_provider = mock_tx_provider_limited_calls(n_calls, input_chunks);

    // The number of times the mempool will be called until timeout is unpredicted.
    add_limitless_empty_calls(&mut mock_tx_provider);
    mock_tx_provider
}

fn add_limitless_empty_calls(mock_tx_provider: &mut MockTransactionProvider) {
    mock_tx_provider
        .expect_get_txs()
        .with(eq(TX_CHUNK_SIZE))
        .returning(|_n_txs| Ok(NextTxs::Txs(Vec::new())));
}

fn compare_tx_hashes(input: &[Transaction], blockifier_input: &[BlockifierTransaction]) -> bool {
    let expected_tx_hashes: Vec<TransactionHash> = input.iter().map(|tx| tx.tx_hash()).collect();
    let input_tx_hashes: Vec<TransactionHash> =
        blockifier_input.iter().map(BlockifierTransaction::tx_hash).collect();
    expected_tx_hashes == input_tx_hashes
}

async fn verify_build_block_output(
    input_txs: Vec<Transaction>,
    expected_block_len: usize,
    expected_block_artifacts: BlockExecutionArtifacts,
    result_block_artifacts: BlockExecutionArtifacts,
    mut output_stream_receiver: UnboundedReceiver<Transaction>,
) {
    // Verify the transactions in the output channel.
    let mut output_txs = vec![];
    output_stream_receiver.recv_many(&mut output_txs, TX_CHANNEL_SIZE).await;

    assert_eq!(output_txs.len(), expected_block_len);
    for tx in input_txs.iter().take(expected_block_len) {
        assert!(output_txs.contains(tx));
    }

    // Verify the block artifacts.
    assert_eq!(result_block_artifacts, expected_block_artifacts);
}

async fn run_build_block(
    mock_transaction_executor: MockTransactionExecutorTrait,
    tx_provider: MockTransactionProvider,
    output_sender: Option<UnboundedSender<Transaction>>,
    fail_on_err: bool,
) -> BlockBuilderResult<BlockExecutionArtifacts> {
    let block_builder = BlockBuilder::new(Box::new(mock_transaction_executor), TX_CHUNK_SIZE);
    let deadline = tokio::time::Instant::now()
        + tokio::time::Duration::from_secs(BLOCK_GENERATION_DEADLINE_SECS);

    block_builder.build_block(deadline, Box::new(tx_provider), output_sender, fail_on_err).await
}

// TODO: Add test case for failed transaction.
#[rstest]
#[case::one_chunk_block(3, test_txs(0..3), one_chunk_test_expectations(&input_txs))]
#[case::two_chunks_block(6, test_txs(0..6), two_chunks_test_expectations(&input_txs))]
#[case::empty_block(0, vec![], empty_block_test_expectations())]
#[case::block_full(1, test_txs(0..3), block_full_test_expectations(&input_txs, expected_block_size))]
#[case::deadline_reached_after_first_chunk(3, test_txs(0..6), test_expectations_with_delay(&input_txs))]
#[case::stream_done(2, test_txs(0..2), stream_done_test_expectations(&input_txs))]
#[tokio::test]
async fn test_build_block(
    #[case] expected_block_size: usize,
    #[case] input_txs: Vec<Transaction>,
    #[case] test_expectations: (
        MockTransactionExecutorTrait,
        MockTransactionProvider,
        BlockExecutionArtifacts,
    ),
) {
    let (mock_transaction_executor, mock_tx_provider, expected_block_artifacts) = test_expectations;

    let (output_tx_sender, output_tx_receiver) = output_channel();

    let result_block_artifacts =
        run_build_block(mock_transaction_executor, mock_tx_provider, Some(output_tx_sender), false)
            .await
            .unwrap();

    verify_build_block_output(
        input_txs,
        expected_block_size,
        expected_block_artifacts,
        result_block_artifacts,
        output_tx_receiver,
    )
    .await;
}

#[tokio::test]
async fn test_validate_block() {
    let input_txs = test_txs(0..3);
    let (mock_transaction_executor, expected_block_artifacts) =
        one_chunk_mock_executor(&input_txs, input_txs.len());
    let mock_tx_provider = mock_tx_provider_stream_done(input_txs);

    let result_block_artifacts =
        run_build_block(mock_transaction_executor, mock_tx_provider, None, true).await.unwrap();

    assert_eq!(result_block_artifacts, expected_block_artifacts);
}

#[tokio::test]
async fn test_validate_block_with_error() {
    let input_txs = test_txs(0..3);
    let expected_block_size = 1;
    let (mock_transaction_executor, mock_tx_provider, _) =
        block_full_test_expectations(&input_txs, expected_block_size);

    let result =
        run_build_block(mock_transaction_executor, mock_tx_provider, None, true).await.unwrap_err();

    assert_matches!(
        result,
        BlockBuilderError::FailOnError(BlockifierTransactionExecutorError::BlockFull)
    );
}
