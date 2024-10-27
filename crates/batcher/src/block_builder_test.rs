use std::sync::Arc;

use blockifier::blockifier::transaction_executor::{
    TransactionExecutorError,
    TransactionExecutorResult,
};
use blockifier::bouncer::BouncerWeights;
use blockifier::transaction::objects::TransactionExecutionInfo;
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use indexmap::IndexMap;
use mockall::predicate::eq;
use rstest::{fixture, rstest};
use starknet_api::executable_transaction::Transaction;
use starknet_api::felt;
use starknet_api::transaction::TransactionHash;
use starknet_mempool_types::communication::MockMempoolClient;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::block_builder::{BlockBuilder, BlockBuilderTrait, BlockExecutionArtifacts};
use crate::test_utils::test_txs;
use crate::transaction_executor::MockTransactionExecutorTrait;

const TEST_DEADLINE_SECS: u64 = 1;
const TEST_CHANNEL_SIZE: usize = 50;

#[fixture]
fn output_channel() -> (UnboundedSender<Transaction>, UnboundedReceiver<Transaction>) {
    tokio::sync::mpsc::unbounded_channel()
}

// TODO(yael 22/9/2024): add a test case where some transactions fail to execute.
#[rstest]
#[case::one_chunk_block(5, 5, 5, false, None)]
#[case::multiple_chunks_block(10, 2, 10, false, None)]
#[case::empty_block(0, 5, 0, false, None)]
#[case::deadline_after_first_chunk(6, 3, 3, true, None)]
#[case::block_full(6, 3, 4, false, Some(4))]
#[tokio::test]
async fn test_build_block(
    #[case] input_txs_len: usize,
    #[case] execution_chunk_size: usize,
    #[case] expected_block_len: usize,
    #[case] enable_execution_delay: bool,
    #[case] block_full_tx_index: Option<usize>,
    output_channel: (UnboundedSender<Transaction>, UnboundedReceiver<Transaction>),
) {
    let (output_sender, mut output_stream_receiver) = output_channel;

    // Create the input transactions.
    let input_txs = test_txs(0..input_txs_len);

    // Create the mock transaction executor, mock mempool client and the expected block artifacts.
    let (mock_transaction_executor, mock_mempool_client, expected_block_artifacts) =
        set_transaction_executor_expectations(
            &input_txs,
            expected_block_len,
            execution_chunk_size,
            enable_execution_delay,
            block_full_tx_index,
        );

    // Build the block.
    let block_builder =
        BlockBuilder::new(Box::new(mock_transaction_executor), execution_chunk_size);
    let deadline =
        tokio::time::Instant::now() + tokio::time::Duration::from_secs(TEST_DEADLINE_SECS);
    let result_block_artifacts = block_builder
        .build_block(deadline, Arc::new(mock_mempool_client), output_sender)
        .await
        .unwrap();

    // Check the transactions in the output channel.
    let mut output_txs = vec![];
    output_stream_receiver.recv_many(&mut output_txs, TEST_CHANNEL_SIZE).await;

    assert_eq!(output_txs.len(), expected_block_len);
    for tx in input_txs.iter().take(expected_block_len) {
        assert!(output_txs.contains(tx));
    }

    // Check the block artifacts.
    assert_eq!(result_block_artifacts, expected_block_artifacts);
}

fn set_transaction_executor_expectations(
    input_txs: &[Transaction],
    num_txs_to_execute: usize,
    chunk_size: usize,
    enable_execution_delay: bool,
    block_full_tx_index: Option<usize>,
) -> (MockTransactionExecutorTrait, MockMempoolClient, BlockExecutionArtifacts) {
    let input_chunks: Vec<Vec<Transaction>> =
        input_txs.chunks(chunk_size).map(|chunk| chunk.to_vec()).collect();
    let number_of_chunks = match enable_execution_delay {
        true => 1,
        false => input_txs.len().div_ceil(chunk_size),
    };

    let mock_mempool_client =
        mock_mempool_client(number_of_chunks, chunk_size, input_chunks.clone());

    let output_block_artifacts =
        block_builder_expected_output(num_txs_to_execute, block_full_tx_index);
    let expected_block_artifacts = output_block_artifacts.clone();

    let mock_transaction_executor = mock_transaction_executor(
        input_txs,
        number_of_chunks,
        chunk_size,
        block_full_tx_index,
        enable_execution_delay,
        input_chunks,
        output_block_artifacts,
    );

    (mock_transaction_executor, mock_mempool_client, expected_block_artifacts)
}

fn mock_mempool_client(
    number_of_chunks: usize,
    chunk_size: usize,
    mut input_chunks_copy: Vec<Vec<Transaction>>,
) -> MockMempoolClient {
    let mut mock_mempool_client = MockMempoolClient::new();
    mock_mempool_client
        .expect_get_txs()
        .times(number_of_chunks)
        .with(eq(chunk_size))
        .returning(move |_n_txs| Ok(input_chunks_copy.remove(0)));
    // The number of times the mempool will be called until timeout is unpredicted.
    mock_mempool_client.expect_get_txs().with(eq(chunk_size)).returning(|_n_txs| Ok(Vec::new()));
    mock_mempool_client
}

fn mock_transaction_executor(
    input_txs: &[Transaction],
    number_of_chunks: usize,
    chunk_size: usize,
    block_full_tx_index: Option<usize>,
    enable_execution_delay: bool,
    mut input_chunks: Vec<Vec<Transaction>>,
    output_block_artifacts: BlockExecutionArtifacts,
) -> MockTransactionExecutorTrait {
    let mut mock_transaction_executor = MockTransactionExecutorTrait::new();

    let mut chunk_count = 0;
    let output_size = input_txs.len();
    mock_transaction_executor.expect_add_txs_to_block().times(number_of_chunks).returning(
        move |executor_input| {
            let result =
                generate_executor_output(chunk_count, chunk_size, output_size, block_full_tx_index);
            chunk_count += 1;
            if enable_execution_delay {
                std::thread::sleep(std::time::Duration::from_secs(TEST_DEADLINE_SECS));
            }
            // Check that the input to the executor is correct.
            compare_tx_hashes(&mut input_chunks, executor_input);
            result
        },
    );

    mock_transaction_executor.expect_close_block().return_once(move || {
        Ok((
            output_block_artifacts.commitment_state_diff,
            output_block_artifacts.visited_segments_mapping,
            output_block_artifacts.bouncer_weights,
        ))
    });
    mock_transaction_executor
}

fn generate_executor_output(
    chunk_number: usize,
    chunk_size: usize,
    input_size: usize,
    block_full_tx_index: Option<usize>,
) -> Vec<TransactionExecutorResult<TransactionExecutionInfo>> {
    let mut executor_output = vec![];
    let start = chunk_number * chunk_size;
    let end = std::cmp::min(start + chunk_size, input_size);
    for i in start..end {
        if block_full_tx_index == Some(i) {
            executor_output.push(Err(TransactionExecutorError::BlockFull));
            break;
        }
        executor_output.push(Ok(execution_info()));
    }
    executor_output
}

fn compare_tx_hashes(input_chunks: &mut Vec<Vec<Transaction>>, input: &[BlockifierTransaction]) {
    let expected_tx_hashes: Vec<TransactionHash> =
        input_chunks.remove(0).iter().map(|tx| tx.tx_hash()).collect();
    let input_tx_hashes: Vec<TransactionHash> =
        input.iter().map(BlockifierTransaction::tx_hash).collect();
    assert_eq!(expected_tx_hashes, input_tx_hashes);
}

// Fill the executor outputs with some non-default values to make sure the block_builder uses them.
fn block_builder_expected_output(
    execution_info_len: usize,
    block_full_tx_index: Option<usize>,
) -> BlockExecutionArtifacts {
    let mut execution_infos_mapping = IndexMap::new();
    for i in 0..execution_info_len {
        if block_full_tx_index == Some(i) {
            break;
        }
        let tx_hash = TransactionHash(felt!(u8::try_from(i).unwrap()));
        execution_infos_mapping.insert(tx_hash, execution_info());
    }

    BlockExecutionArtifacts {
        execution_infos: execution_infos_mapping,
        bouncer_weights: BouncerWeights { gas: 100, ..BouncerWeights::empty() },
        commitment_state_diff: Default::default(),
        visited_segments_mapping: Default::default(),
    }
}

fn execution_info() -> TransactionExecutionInfo {
    TransactionExecutionInfo { revert_error: Some("Test string".to_string()), ..Default::default() }
}
