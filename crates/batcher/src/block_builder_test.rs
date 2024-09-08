use blockifier::blockifier::transaction_executor::{
    TransactionExecutorError,
    TransactionExecutorResult,
};
use blockifier::bouncer::BouncerWeights;
use blockifier::transaction::objects::TransactionExecutionInfo;
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use indexmap::IndexMap;
use rstest::{fixture, rstest};
use starknet_api::executable_transaction::Transaction;
use starknet_api::felt;
use starknet_api::transaction::TransactionHash;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::block_builder::{
    BlockBuilder,
    BlockBuilderTrait,
    BlockExecutionArtifacts,
    MockTransactionExecutorTrait,
};
use crate::test_utils::test_txs;

const TEST_DEADLINE_SECS: u64 = 1;
const TEST_CHANNEL_SIZE: usize = 50;

#[fixture]
fn input_channel() -> (mpsc::Sender<Transaction>, ReceiverStream<Transaction>) {
    let (input_sender, input_receiver) = mpsc::channel::<Transaction>(TEST_CHANNEL_SIZE);
    let input_tx_stream = ReceiverStream::new(input_receiver);
    (input_sender, input_tx_stream)
}

#[fixture]
fn output_channel() -> (mpsc::Sender<Transaction>, mpsc::Receiver<Transaction>) {
    mpsc::channel::<Transaction>(TEST_CHANNEL_SIZE)
}

#[fixture]
fn input_txs(#[default(1)] input_txs_len: usize) -> Vec<Transaction> {
    test_txs(0..input_txs_len)
}

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
    input_channel: (mpsc::Sender<Transaction>, ReceiverStream<Transaction>),
    output_channel: (mpsc::Sender<Transaction>, mpsc::Receiver<Transaction>),
) {
    let (input_sender, input_receiver) = input_channel;
    let (output_sender, mut output_stream_receiver) = output_channel;

    // Create the input transactions and send them to the input channel.
    let input_txs = test_txs(0..input_txs_len);
    for tx in input_txs.iter() {
        input_sender.send(tx.clone()).await.unwrap();
    }

    // Create the mock transaction executor and the expected block artifacts.
    let (mock_transaction_executor, expected_block_artifacts) =
        set_transaction_executor_expectations(
            &input_txs,
            expected_block_len,
            execution_chunk_size,
            enable_execution_delay,
            block_full_tx_index,
        );

    // Build the block.
    let mut block_builder =
        BlockBuilder::new(Box::new(mock_transaction_executor), execution_chunk_size);
    let deadline =
        tokio::time::Instant::now() + tokio::time::Duration::from_secs(TEST_DEADLINE_SECS);
    let result_block_artifacts =
        block_builder.build_block(deadline, input_receiver, output_sender).await.unwrap();

    // Check the transaction in the output channel.
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
) -> (MockTransactionExecutorTrait, BlockExecutionArtifacts) {
    let mut input_chunks: Vec<Vec<Transaction>> =
        input_txs.chunks(chunk_size).map(|chunk| chunk.to_vec()).collect();
    let output_block_artifacts =
        block_builder_expected_output(num_txs_to_execute, block_full_tx_index);
    let expected_block_artifacts = output_block_artifacts.clone();

    let mut mock_transaction_executor = MockTransactionExecutorTrait::new();

    let number_of_chunks = match enable_execution_delay {
        true => 1,
        false => input_txs.len().div_ceil(chunk_size),
    };
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
    (mock_transaction_executor, expected_block_artifacts)
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
        bouncer_weights: BouncerWeights { gas: 100, ..Default::default() },
        ..Default::default()
    }
}

fn execution_info() -> TransactionExecutionInfo {
    TransactionExecutionInfo { revert_error: Some("Test string".to_string()), ..Default::default() }
}

// TODO(yael 22/9/2024): add more tests cases for the block builder: test where some transactions
// fail.
