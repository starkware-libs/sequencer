
use starknet_api::executable_transaction::Transaction;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::block_builder::{BlockBuilder, BlockBuilderTrait, BlockExecutionArtifacts};
use crate::transaction_executor::MockTransactionExecutorTrait;
use crate::transaction_provider::MockTransactionProvider;

const BLOCK_GENERATION_DEADLINE_SECS: u64 = 1;
const TX_CHANNEL_SIZE: usize = 50;
const TX_CHUNK_SIZE: usize = 3;

fn output_channel() -> (UnboundedSender<Transaction>, UnboundedReceiver<Transaction>) {
    tokio::sync::mpsc::unbounded_channel()
}

// TODO: enable the test  and remove all '#[allow(dead_code)]' once it is fully implemented.
// TODO: Add test cases for one_chunk, multiple chunks, block full, empty block, failed transaction,
// timeout reached.
// #[rstest]
// #[case::one_chunk_block(3, test_txs(0..3), one_chunk_test_expectations(&input_txs))]
// #[tokio::test]
#[allow(dead_code)]
async fn test_build_block(
    expected_block_size: usize,
    input_txs: Vec<Transaction>,
    test_expectations: (MockTransactionExecutorTrait, MockTransactionProvider, BlockExecutionArtifacts),
) {
    let (mock_transaction_executor, mock_tx_provider, expected_block_artifacts) =
        test_expectations;

    let (output_tx_sender, output_tx_receiver) = output_channel();

    let result_block_artifacts =
        run_build_block(mock_transaction_executor, mock_tx_provider, output_tx_sender).await;

    verify_build_block_output(
        input_txs,
        expected_block_size,
        expected_block_artifacts,
        result_block_artifacts,
        output_tx_receiver,
    )
    .await;
}

#[allow(dead_code)]
fn one_chunk_test_expectations(
    _input_txs: &[Transaction],
) -> (MockTransactionExecutorTrait, MockTransactionProvider, BlockExecutionArtifacts) {
    todo!();
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
    output_sender: UnboundedSender<Transaction>,
) -> BlockExecutionArtifacts {
    let block_builder = BlockBuilder::new(Box::new(mock_transaction_executor), TX_CHUNK_SIZE);
    let deadline = tokio::time::Instant::now()
        + tokio::time::Duration::from_secs(BLOCK_GENERATION_DEADLINE_SECS);

    block_builder.build_block(deadline, Box::new(tx_provider), output_sender).await.unwrap()
}
