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
    println!("input_txs_len: {}", input_txs_len);
    test_txs(0..input_txs_len)
}

#[rstest]
#[case::one_chunk_block(5, 5, 4, 0)]
#[case::multiple_chunks_block(10, 2, 9, 0)]
#[case::empty_block(0, 5, 0, 0)]
#[case::deadline_after_first_chunk(6, 3, 3, 1)]
#[tokio::test]
async fn test_build_block(
    #[case] input_txs_len: usize,
    #[case] execution_chunk_size: usize,
    #[case] expected_block_len: usize,
    #[case] execution_delay_secs: u64,
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
            execution_delay_secs,
        );

    // Build the block.
    let mut block_builder =
        BlockBuilder::new(Box::new(mock_transaction_executor), execution_chunk_size);
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(1);
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
    execution_delay_secs: u64,
) -> (MockTransactionExecutorTrait, BlockExecutionArtifacts) {
    let mut input_chunks: Vec<Vec<Transaction>> =
        input_txs.chunks(chunk_size).map(|chunk| chunk.to_vec()).collect();
    let (output_block_artifacts, mut executor_output) =
        executor_test_outputs(num_txs_to_execute, chunk_size);
    let expected_block_artifacts = output_block_artifacts.clone();

    let mut mock_transaction_executor = MockTransactionExecutorTrait::new();

    let mut number_of_chunks = div_round_up(input_txs.len(), chunk_size);
    // TODO make this variable bool,not secs
    if execution_delay_secs > 0 {
        number_of_chunks = 1;
    }
    mock_transaction_executor.expect_add_txs_to_block().times(number_of_chunks).returning(
        move |executor_input| {
            let result = executor_output.remove(0);
            std::thread::sleep(std::time::Duration::from_secs(execution_delay_secs));
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

fn compare_tx_hashes(input_chunks: &mut Vec<Vec<Transaction>>, input: &[BlockifierTransaction]) {
    let expected_tx_hashes: Vec<TransactionHash> =
        input_chunks.remove(0).iter().map(|tx| tx.tx_hash()).collect();
    let input_tx_hashes: Vec<TransactionHash> =
        input.iter().map(BlockifierTransaction::tx_hash).collect();
    assert_eq!(expected_tx_hashes, input_tx_hashes);
}

// Fill the executor outputs with some non-default values to make sure the block_builder uses them.
fn executor_test_outputs(
    execution_info_len: usize,
    execution_tx_chunk_size: usize,
) -> (BlockExecutionArtifacts, Vec<Vec<TransactionExecutorResult<TransactionExecutionInfo>>>) {
    let mut execution_infos_mapping = IndexMap::new();
    let mut execution_infos_vec: Vec<Vec<TransactionExecutorResult<TransactionExecutionInfo>>> =
        vec![];
    for i in 0..execution_info_len {
        let tx_hash = TransactionHash(felt!(u8::try_from(i).unwrap()));
        let value = TransactionExecutionInfo {
            revert_error: Some("Test string".to_string()),
            ..Default::default()
        };
        execution_infos_mapping.insert(tx_hash, value.clone());

        if i % execution_tx_chunk_size == 0 {
            execution_infos_vec.push(vec![]);
        }

        execution_infos_vec.last_mut().unwrap().push(Ok(value.clone()));
        execution_infos_mapping
            .insert(TransactionHash(felt!(u8::try_from(i).unwrap())), value.clone());
    }

    // Add BlockFull error in order to trigger closing the block.
    if execution_info_len > 0 {
        execution_infos_vec.last_mut().unwrap().push(Err(TransactionExecutorError::BlockFull));
    }

    let block_execution_artifacts = BlockExecutionArtifacts {
        execution_infos: execution_infos_mapping,
        bouncer_weights: BouncerWeights { gas: 100, ..Default::default() },
        ..Default::default()
    };

    (block_execution_artifacts, execution_infos_vec)
}

fn div_round_up(a: usize, b: usize) -> usize {
    (a + b - 1) / b
}

// TODO(yael 22/9/2024): add more tests cases for the block builder: test where some transactions
// fail.
