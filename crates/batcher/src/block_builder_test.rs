use blockifier::bouncer::BouncerWeights;
use blockifier::transaction::objects::TransactionExecutionInfo;
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use indexmap::IndexMap;
use mockall::Sequence;
use rstest::rstest;
use starknet_api::executable_transaction::Transaction;
use starknet_api::felt;
use starknet_api::transaction::TransactionHash;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio_stream::wrappers::ReceiverStream;

use crate::block_builder::{BlockBuilder, BlockBuilderTrait, BlockExecutionArtifacts};
use crate::test_utils::test_txs;
use crate::transaction_executor::MockTransactionExecutorTrait;

const BLOCK_GENERATION_DEADLINE_SECS: u64 = 1;
const TX_CHANNEL_SIZE: usize = 50;
const TX_CHUNK_SIZE: usize = 3;

fn input_channel() -> (mpsc::Sender<Transaction>, ReceiverStream<Transaction>) {
    let (input_sender, input_receiver) = mpsc::channel::<Transaction>(TX_CHANNEL_SIZE);
    let input_tx_stream = ReceiverStream::new(input_receiver);
    (input_sender, input_tx_stream)
}

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

// TODO: Add test cases for block full, empty block, failed transaction,
// timeout reached.
#[rstest]
#[case::one_chunk_block(3, test_txs(0..3), one_chunk_test_expectations(&input_txs))]
#[case::two_chunks_block(6, test_txs(0..6), two_chunks_test_expectations(&input_txs))]
#[tokio::test]
async fn test_build_block(
    #[case] expected_block_size: usize,
    #[case] input_txs: Vec<Transaction>,
    #[case] test_expectations: (MockTransactionExecutorTrait, BlockExecutionArtifacts),
) {
    let (mock_transaction_executor, expected_block_artifacts) = test_expectations;

    let (input_tx_sender, input_tx_receiver) = input_channel();
    let (output_tx_sender, output_tx_receiver) = output_channel();

    // Run build_block and send input transactions in parallel.
    let handle = spawn_build_block(mock_transaction_executor, input_tx_receiver, output_tx_sender);
    for tx in input_txs.iter() {
        input_tx_sender.send(tx.clone()).await.unwrap();
    }
    let result_block_artifacts = handle.await.unwrap();

    verify_build_block_output(
        input_txs,
        expected_block_size,
        expected_block_artifacts,
        result_block_artifacts,
        output_tx_receiver,
    )
    .await;
}

fn one_chunk_test_expectations(
    input_txs: &[Transaction],
) -> (MockTransactionExecutorTrait, BlockExecutionArtifacts) {
    let block_size = input_txs.len();
    let input_txs_cloned = input_txs.to_vec();
    let mut mock_transaction_executor = MockTransactionExecutorTrait::new();

    mock_transaction_executor
        .expect_add_txs_to_block()
        .withf(move |blockifier_input| {
            compare_tx_hashes(input_txs_cloned.clone(), blockifier_input)
        })
        .return_once(move |_| vec![execution_info(); block_size].into_iter().map(Ok).collect());

    let expected_block_artifacts =
        set_close_block_expectations(&mut mock_transaction_executor, block_size);

    (mock_transaction_executor, expected_block_artifacts)
}

fn two_chunks_test_expectations(
    input_txs: &[Transaction],
) -> (MockTransactionExecutorTrait, BlockExecutionArtifacts) {
    let first_chunk = input_txs[..TX_CHUNK_SIZE].to_vec();
    let second_chunk = input_txs[TX_CHUNK_SIZE..].to_vec();
    let block_size = input_txs.len();

    let mut mock_transaction_executor = MockTransactionExecutorTrait::new();
    let mut set_expectation = |tx_chunk: Vec<Transaction>, seq: &mut Sequence| {
        mock_transaction_executor
            .expect_add_txs_to_block()
            .times(1)
            .in_sequence(seq)
            .withf(move |blockifier_input| compare_tx_hashes(tx_chunk.clone(), blockifier_input))
            .returning(move |_| {
                vec![execution_info(); TX_CHUNK_SIZE].into_iter().map(Ok).collect()
            });
    };

    let mut seq = Sequence::new();
    set_expectation(first_chunk, &mut seq);
    set_expectation(second_chunk, &mut seq);

    let expected_block_artifacts =
        set_close_block_expectations(&mut mock_transaction_executor, block_size);

    (mock_transaction_executor, expected_block_artifacts)
}

// Fill the executor outputs with some non-default values to make sure the block_builder uses
// them.
fn block_builder_expected_output(execution_info_len: usize) -> BlockExecutionArtifacts {
    let mut execution_infos_mapping = IndexMap::new();
    (0..execution_info_len).for_each(|i| {
        let tx_hash = TransactionHash(felt!(u8::try_from(i).unwrap()));
        execution_infos_mapping.insert(tx_hash, execution_info());
    });

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

fn compare_tx_hashes(input: Vec<Transaction>, blockifier_input: &[BlockifierTransaction]) -> bool {
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

fn spawn_build_block(
    mock_transaction_executor: MockTransactionExecutorTrait,
    input_receiver: ReceiverStream<Transaction>,
    output_sender: UnboundedSender<Transaction>,
) -> tokio::task::JoinHandle<BlockExecutionArtifacts> {
    let mut block_builder = BlockBuilder::new(Box::new(mock_transaction_executor), TX_CHUNK_SIZE);
    let deadline = tokio::time::Instant::now()
        + tokio::time::Duration::from_secs(BLOCK_GENERATION_DEADLINE_SECS);

    tokio::spawn(async move {
        block_builder.build_block(deadline, input_receiver, output_sender).await.unwrap()
    })
}
