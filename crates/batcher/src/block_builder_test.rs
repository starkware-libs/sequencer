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
fn input_channel() -> (mpsc::Sender<Transaction>, mpsc::Receiver<Transaction>) {
    mpsc::channel::<Transaction>(TEST_CHANNEL_SIZE)
}

#[fixture]
fn output_channel() -> (mpsc::Sender<Transaction>, mpsc::Receiver<Transaction>) {
    mpsc::channel::<Transaction>(TEST_CHANNEL_SIZE)
}

#[rstest]
#[case::one_chunk_block(5, 5, 2, 0)]
#[case::nultiple_chunks_block(10, 2, 9, 0)]
#[case::empty_block(0, 5, 0, 0)]
#[case::deadline_after_first_chunk(6, 3, 3, 1)]
#[tokio::test]
async fn test_build_block(
    #[case] input_txs_len: usize,
    #[case] executor_tx_chunk_len: usize,
    #[case] expected_output_txs_len: usize,
    #[case] add_execution_delay_secs: u64,
    input_channel: (mpsc::Sender<Transaction>, mpsc::Receiver<Transaction>),
    output_channel: (mpsc::Sender<Transaction>, mpsc::Receiver<Transaction>),
) {
    // The last transaction is not included in the block, since we return BlockFull for it.
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(1);
    let (input_channel_sender, input_channel_receiver) = input_channel;
    let (output_stream_sender, mut output_stream_receiver) = output_channel;
    let input_tx_stream = ReceiverStream::new(input_channel_receiver);

    // Create the input transactions and send them to the input channel.
    let txs = test_txs(0..input_txs_len);
    for tx in txs.iter() {
        input_channel_sender.send(tx.clone()).await.unwrap();
    }

    // Create the mock transaction executor and the expected block artifacts.
    let (mock_transaction_executor, expected_block_artifacts) = get_mock_transaction_executor(
        &txs,
        expected_output_txs_len,
        executor_tx_chunk_len,
        add_execution_delay_secs,
    );

    // Build the block.
    let mut block_builder =
        BlockBuilder::new(Box::new(mock_transaction_executor), executor_tx_chunk_len);
    let result_block_artifacts =
        block_builder.build_block(deadline, input_tx_stream, output_stream_sender).await.unwrap();

    // Check the transaction in the output channel.
    let mut output_txs = vec![];
    while let Some(tx) = output_stream_receiver.recv().await {
        output_txs.push(tx);
    }
    assert_eq!(output_txs.len(), expected_output_txs_len);
    for tx in txs[0..expected_output_txs_len.checked_sub(1).unwrap_or_default()].iter() {
        assert!(output_txs.contains(tx));
    }

    // Check the block artifacts.
    assert_eq!(result_block_artifacts, expected_block_artifacts);
}

fn get_mock_transaction_executor(
    input_txs: &[Transaction],
    output_txs_len: usize,
    executor_tx_chunk_len: usize,
    add_delay_secs: u64,
) -> (MockTransactionExecutorTrait, BlockExecutionArtifacts) {
    let mut input_chunks: Vec<Vec<Transaction>> =
        input_txs.chunks(executor_tx_chunk_len).map(|chunk| chunk.to_vec()).collect();
    let (output_block_artifacts, mut executor_output) =
        executor_test_outputs(output_txs_len, executor_tx_chunk_len);
    let expected_block_artifacts = output_block_artifacts.clone();

    let mut mock_transaction_executor = MockTransactionExecutorTrait::new();

    mock_transaction_executor.expect_add_txs_to_block().returning(move |executor_input| {
        let result = executor_output.remove(0);
        std::thread::sleep(std::time::Duration::from_secs(add_delay_secs));
        // Check that the input to the executor is correct.
        compare_tx_hashes(&mut input_chunks, executor_input);
        result
    });

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

// TODO(yael 22/9/2024): add more tests cases for the block builder: test where some transactions
// fail.
