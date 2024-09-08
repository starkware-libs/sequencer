use blockifier::blockifier::transaction_executor::{
    TransactionExecutorError,
    TransactionExecutorResult,
};
use blockifier::bouncer::BouncerWeights;
use blockifier::transaction::objects::TransactionExecutionInfo;
use indexmap::IndexMap;
use rstest::rstest;
use starknet_api::executable_transaction::Transaction;
use starknet_api::felt;
use starknet_api::transaction::TransactionHash;
use tokio_stream::wrappers::ReceiverStream;

use crate::block_builder::{
    BlockBuilder,
    BlockBuilderTrait,
    BlockExecutionArtifacts,
    MockTransactionExecutorTrait,
};
use crate::test_utils::test_txs;

#[rstest]
#[tokio::test]
async fn test_build_block() {
    const INPUT_TXS_LEN: usize = 3;
    // The output txs_bufer len will be one less than the input, since the tx one will get a block
    // full error.
    const OUTPUT_TXS_LEN: usize = INPUT_TXS_LEN - 1;
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(1);
    let (mempool_tx_sender, mempool_tx_receiver) =
        tokio::sync::mpsc::channel::<Transaction>(INPUT_TXS_LEN);
    let mempool_tx_stream = ReceiverStream::new(mempool_tx_receiver);
    let (output_stream_sender, mut output_stream_receiver) =
        tokio::sync::mpsc::channel::<Transaction>(OUTPUT_TXS_LEN);

    let txs = test_txs(0..INPUT_TXS_LEN);
    for tx in txs.iter() {
        mempool_tx_sender.send(tx.clone()).await.unwrap();
    }
    let (output_block_artifacts, executor_output) = executor_test_outputs(OUTPUT_TXS_LEN);
    let expected_block_artifacts = output_block_artifacts.clone();
    let mut mock_transaction_executor = MockTransactionExecutorTrait::new();
    mock_transaction_executor.expect_add_txs_to_block().return_once(|_| executor_output);
    mock_transaction_executor.expect_close_block().return_once(move || {
        Ok((
            output_block_artifacts.commitment_state_diff,
            output_block_artifacts.visited_segments_mapping,
            output_block_artifacts.bouncer_weights,
        ))
    });
    let mut block_builder = BlockBuilder::new(Box::new(mock_transaction_executor), INPUT_TXS_LEN);
    let result_block_artifacts =
        block_builder.build_block(deadline, mempool_tx_stream, output_stream_sender).await.unwrap();
    let mut output_txs = vec![];
    while let Some(tx) = output_stream_receiver.recv().await {
        output_txs.push(tx);
    }
    assert_eq!(output_txs.len(), OUTPUT_TXS_LEN);
    for tx in txs[0..txs.len() - 1].iter() {
        assert!(output_txs.contains(tx));
    }
    assert_eq!(result_block_artifacts, expected_block_artifacts);
}

// Fill the executor outputs with some non-default values to make sure the block_builder uses them.
fn executor_test_outputs(
    execution_info_len: usize,
) -> (BlockExecutionArtifacts, Vec<TransactionExecutorResult<TransactionExecutionInfo>>) {
    let mut execution_infos_mapping = IndexMap::new();
    let mut execution_infos_vec = Vec::new();
    for i in 0..execution_info_len {
        let value = TransactionExecutionInfo {
            revert_error: Some("Test string".to_string()),
            ..Default::default()
        };
        execution_infos_mapping
            .insert(TransactionHash(felt!(u8::try_from(i).unwrap())), value.clone());
        execution_infos_vec.push(Ok(value));
    }
    execution_infos_vec.push(Err(TransactionExecutorError::BlockFull));
    let block_execution_artifacts = BlockExecutionArtifacts {
        execution_infos: execution_infos_mapping,
        bouncer_weights: BouncerWeights { gas: 100, ..Default::default() },
        ..Default::default()
    };
    (block_execution_artifacts, execution_infos_vec)
}

// TODO(yael 22/9/2024): add more tests cases for the block builder: negative tests, multiple chunk
// blocks, etc.
