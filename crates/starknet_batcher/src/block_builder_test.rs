use std::collections::HashSet;
use std::sync::Arc;

use assert_matches::assert_matches;
use blockifier::blockifier::transaction_executor::{
    BlockExecutionSummary,
    TransactionExecutorError,
};
use blockifier::bouncer::BouncerWeights;
use blockifier::fee::fee_checks::FeeCheckError;
use blockifier::fee::receipt::TransactionReceipt;
use blockifier::state::errors::StateError;
use blockifier::transaction::objects::{RevertError, TransactionExecutionInfo};
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use indexmap::{indexmap, IndexMap};
use mockall::predicate::eq;
use mockall::Sequence;
use rstest::rstest;
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::core::ChainId;
use starknet_api::execution_resources::{GasAmount, GasVector};
use starknet_api::transaction::fields::Fee;
use starknet_api::transaction::TransactionHash;
use starknet_api::tx_hash;
use starknet_class_manager_types::transaction_converter::TransactionConverter;
use starknet_class_manager_types::MockClassManagerClient;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::block_builder::{
    BlockBuilder,
    BlockBuilderError,
    BlockBuilderExecutionParams,
    BlockBuilderResult,
    BlockBuilderTrait,
    BlockExecutionArtifacts,
    FailOnErrorCause,
};
use crate::test_utils::test_txs;
use crate::transaction_executor::MockTransactionExecutorTrait;
use crate::transaction_provider::{MockTransactionProvider, NextTxs};

const BLOCK_GENERATION_DEADLINE_SECS: u64 = 1;
const BLOCK_GENERATION_LONG_DEADLINE_SECS: u64 = 5;
const TX_CHANNEL_SIZE: usize = 50;
const TX_CHUNK_SIZE: usize = 3;

struct TestExpectations {
    mock_transaction_executor: MockTransactionExecutorTrait,
    mock_tx_provider: MockTransactionProvider,
    expected_block_artifacts: BlockExecutionArtifacts,
    expected_txs_output: Vec<InternalConsensusTransaction>,
}

fn output_channel()
-> (UnboundedSender<InternalConsensusTransaction>, UnboundedReceiver<InternalConsensusTransaction>)
{
    tokio::sync::mpsc::unbounded_channel()
}

fn block_execution_artifacts(
    execution_infos: IndexMap<TransactionHash, TransactionExecutionInfo>,
    rejected_tx_hashes: HashSet<TransactionHash>,
) -> BlockExecutionArtifacts {
    let l2_gas_used = GasAmount(execution_infos.len().try_into().unwrap());
    BlockExecutionArtifacts {
        execution_infos,
        rejected_tx_hashes,
        commitment_state_diff: Default::default(),
        compressed_state_diff: Default::default(),
        bouncer_weights: BouncerWeights { l1_gas: 100, ..BouncerWeights::empty() },
        // Each mock transaction uses 1 L2 gas so the total amount should be the number of txs.
        l2_gas_used,
    }
}

// Filling the execution_info with some non-default values to make sure the block_builder uses them.
fn execution_info() -> TransactionExecutionInfo {
    TransactionExecutionInfo {
        revert_error: Some(RevertError::PostExecution(FeeCheckError::MaxFeeExceeded {
            max_fee: Fee(100),
            actual_fee: Fee(101),
        })),
        receipt: TransactionReceipt {
            gas: GasVector { l2_gas: GasAmount(1), ..Default::default() },
            ..Default::default()
        },
        ..Default::default()
    }
}

fn one_chunk_test_expectations() -> TestExpectations {
    let input_txs = test_txs(0..3);
    let block_size = input_txs.len();
    let (mock_transaction_executor, expected_block_artifacts) =
        one_chunk_mock_executor(&input_txs, block_size);

    let mock_tx_provider = mock_tx_provider_limitless_calls(1, vec![input_txs.clone()]);

    TestExpectations {
        mock_transaction_executor,
        mock_tx_provider,
        expected_block_artifacts,
        expected_txs_output: input_txs,
    }
}

fn one_chunk_mock_executor(
    input_txs: &[InternalConsensusTransaction],
    block_size: usize,
) -> (MockTransactionExecutorTrait, BlockExecutionArtifacts) {
    let input_txs_cloned = input_txs.to_vec();
    let mut mock_transaction_executor = MockTransactionExecutorTrait::new();

    mock_transaction_executor
        .expect_add_txs_to_block()
        .times(1)
        .withf(move |blockifier_input| compare_tx_hashes(&input_txs_cloned, blockifier_input))
        .return_once(move |_| (0..block_size).map(|_| Ok(execution_info())).collect());

    let expected_block_artifacts =
        set_close_block_expectations(&mut mock_transaction_executor, block_size);
    (mock_transaction_executor, expected_block_artifacts)
}

fn two_chunks_test_expectations() -> TestExpectations {
    let input_txs = test_txs(0..6);
    let first_chunk = input_txs[..TX_CHUNK_SIZE].to_vec();
    let second_chunk = input_txs[TX_CHUNK_SIZE..].to_vec();
    let block_size = input_txs.len();

    let mut mock_transaction_executor = MockTransactionExecutorTrait::new();
    let mut mock_add_txs_to_block = |tx_chunk: Vec<InternalConsensusTransaction>,
                                     seq: &mut Sequence| {
        mock_transaction_executor
            .expect_add_txs_to_block()
            .times(1)
            .in_sequence(seq)
            .withf(move |blockifier_input| compare_tx_hashes(&tx_chunk, blockifier_input))
            .return_once(move |_| (0..TX_CHUNK_SIZE).map(move |_| Ok(execution_info())).collect());
    };

    let mut seq = Sequence::new();
    mock_add_txs_to_block(first_chunk.clone(), &mut seq);
    mock_add_txs_to_block(second_chunk.clone(), &mut seq);

    let expected_block_artifacts =
        set_close_block_expectations(&mut mock_transaction_executor, block_size);

    let mock_tx_provider = mock_tx_provider_limitless_calls(2, vec![first_chunk, second_chunk]);

    TestExpectations {
        mock_transaction_executor,
        mock_tx_provider,
        expected_block_artifacts,
        expected_txs_output: input_txs,
    }
}

fn empty_block_test_expectations() -> TestExpectations {
    let mut mock_transaction_executor = MockTransactionExecutorTrait::new();
    mock_transaction_executor.expect_add_txs_to_block().times(0);

    let expected_block_artifacts = set_close_block_expectations(&mut mock_transaction_executor, 0);

    let mock_tx_provider = mock_tx_provider_limitless_calls(1, vec![vec![]]);

    TestExpectations {
        mock_transaction_executor,
        mock_tx_provider,
        expected_block_artifacts,
        expected_txs_output: vec![],
    }
}

fn mock_transaction_executor_block_full(
    input_txs: &[InternalConsensusTransaction],
) -> MockTransactionExecutorTrait {
    let input_txs_cloned = input_txs.to_vec();
    let mut mock_transaction_executor = MockTransactionExecutorTrait::new();
    let execution_results = vec![Ok(execution_info())];
    // When the block is full, the executor will return less results than the number of input txs.
    assert!(input_txs.len() > execution_results.len());
    mock_transaction_executor
        .expect_add_txs_to_block()
        .times(1)
        .withf(move |blockifier_input| compare_tx_hashes(&input_txs_cloned, blockifier_input))
        .return_once(move |_| execution_results);
    mock_transaction_executor
}

fn block_full_test_expectations() -> TestExpectations {
    let input_txs = test_txs(0..3);
    let mut mock_transaction_executor = mock_transaction_executor_block_full(&input_txs);

    let expected_block_artifacts = set_close_block_expectations(&mut mock_transaction_executor, 1);

    let mock_tx_provider = mock_tx_provider_limited_calls(1, vec![input_txs.clone()]);

    TestExpectations {
        mock_transaction_executor,
        mock_tx_provider,
        expected_block_artifacts,
        expected_txs_output: vec![input_txs[0].clone()],
    }
}

fn mock_transaction_executor_with_delay(
    input_txs: &[InternalConsensusTransaction],
) -> MockTransactionExecutorTrait {
    let input_txs_cloned = input_txs.to_vec();
    let mut mock_transaction_executor = MockTransactionExecutorTrait::new();
    mock_transaction_executor
        .expect_add_txs_to_block()
        .times(1)
        .withf(move |blockifier_input| compare_tx_hashes(&input_txs_cloned, blockifier_input))
        .return_once(move |_| {
            std::thread::sleep(std::time::Duration::from_secs(BLOCK_GENERATION_DEADLINE_SECS));
            (0..TX_CHUNK_SIZE).map(move |_| Ok(execution_info())).collect()
        });
    mock_transaction_executor
}

fn test_expectations_with_delay() -> TestExpectations {
    let input_txs = test_txs(0..6);
    let first_chunk = input_txs[0..TX_CHUNK_SIZE].to_vec();
    let mut mock_transaction_executor = mock_transaction_executor_with_delay(&first_chunk);

    let expected_block_artifacts =
        set_close_block_expectations(&mut mock_transaction_executor, TX_CHUNK_SIZE);

    let mock_tx_provider = mock_tx_provider_limited_calls(1, vec![first_chunk.clone()]);

    TestExpectations {
        mock_transaction_executor,
        mock_tx_provider,
        expected_block_artifacts,
        expected_txs_output: first_chunk,
    }
}

fn stream_done_test_expectations() -> TestExpectations {
    let input_txs = test_txs(0..2);
    let block_size = input_txs.len();
    let input_txs_cloned = input_txs.clone();
    let mut mock_transaction_executor = MockTransactionExecutorTrait::new();

    mock_transaction_executor
        .expect_add_txs_to_block()
        .times(1)
        .withf(move |blockifier_input| compare_tx_hashes(&input_txs_cloned, blockifier_input))
        .return_once(move |_| (0..block_size).map(|_| Ok(execution_info())).collect());

    let expected_block_artifacts =
        set_close_block_expectations(&mut mock_transaction_executor, block_size);

    let mock_tx_provider = mock_tx_provider_stream_done(input_txs.clone());

    TestExpectations {
        mock_transaction_executor,
        mock_tx_provider,
        expected_block_artifacts,
        expected_txs_output: input_txs,
    }
}

fn transaction_failed_test_expectations() -> TestExpectations {
    let input_txs = test_txs(0..3);

    let mut expected_txs_output = input_txs.clone();
    expected_txs_output.remove(1);

    let mut mock_transaction_executor = MockTransactionExecutorTrait::new();
    let execution_error =
        TransactionExecutorError::StateError(StateError::OutOfRangeContractAddress);
    mock_transaction_executor.expect_add_txs_to_block().times(1).return_once(move |_| {
        vec![Ok(execution_info()), Err(execution_error), Ok(execution_info())]
    });

    let execution_infos_mapping = indexmap![
        tx_hash!(0)=> execution_info(),
        tx_hash!(2)=> execution_info(),
    ];
    let expected_block_artifacts =
        block_execution_artifacts(execution_infos_mapping, vec![tx_hash!(1)].into_iter().collect());
    let expected_block_artifacts_copy = expected_block_artifacts.clone();
    mock_transaction_executor.expect_close_block().times(1).return_once(move || {
        Ok(BlockExecutionSummary {
            state_diff: expected_block_artifacts_copy.commitment_state_diff,
            compressed_state_diff: None,
            bouncer_weights: expected_block_artifacts_copy.bouncer_weights,
        })
    });

    let mock_tx_provider = mock_tx_provider_limitless_calls(1, vec![input_txs]);

    TestExpectations {
        mock_transaction_executor,
        mock_tx_provider,
        expected_block_artifacts,
        expected_txs_output,
    }
}

// Fill the executor outputs with some non-default values to make sure the block_builder uses
// them.
fn block_builder_expected_output(execution_info_len: usize) -> BlockExecutionArtifacts {
    let execution_info_len_u8 = u8::try_from(execution_info_len).unwrap();
    let execution_infos_mapping =
        (0..execution_info_len_u8).map(|i| (tx_hash!(i), execution_info())).collect();
    block_execution_artifacts(execution_infos_mapping, Default::default())
}

fn set_close_block_expectations(
    mock_transaction_executor: &mut MockTransactionExecutorTrait,
    block_size: usize,
) -> BlockExecutionArtifacts {
    let output_block_artifacts = block_builder_expected_output(block_size);
    let output_block_artifacts_copy = output_block_artifacts.clone();
    mock_transaction_executor.expect_close_block().times(1).return_once(move || {
        Ok(BlockExecutionSummary {
            state_diff: output_block_artifacts.commitment_state_diff,
            compressed_state_diff: None,
            bouncer_weights: output_block_artifacts.bouncer_weights,
        })
    });
    output_block_artifacts_copy
}

/// Create a mock tx provider that will return the input chunks for number of chunks queries.
/// This function assumes constant chunk size of TX_CHUNK_SIZE.
fn mock_tx_provider_limited_calls(
    n_calls: usize,
    mut input_chunks: Vec<Vec<InternalConsensusTransaction>>,
) -> MockTransactionProvider {
    let mut mock_tx_provider = MockTransactionProvider::new();
    mock_tx_provider
        .expect_get_txs()
        .times(n_calls)
        .with(eq(TX_CHUNK_SIZE))
        .returning(move |_n_txs| Ok(NextTxs::Txs(input_chunks.remove(0))));
    mock_tx_provider
}

fn mock_tx_provider_stream_done(
    input_chunk: Vec<InternalConsensusTransaction>,
) -> MockTransactionProvider {
    let mut mock_tx_provider = MockTransactionProvider::new();
    let mut seq = Sequence::new();
    mock_tx_provider
        .expect_get_txs()
        .times(1)
        .in_sequence(&mut seq)
        .with(eq(TX_CHUNK_SIZE))
        .return_once(move |_n_txs| Ok(NextTxs::Txs(input_chunk)));
    mock_tx_provider
        .expect_get_txs()
        .times(1)
        .in_sequence(&mut seq)
        .return_once(|_n_txs| Ok(NextTxs::End));
    mock_tx_provider
}

/// Create a mock tx provider client that will return the input chunks and then empty chunks.
/// This function assumes constant chunk size of TX_CHUNK_SIZE.
fn mock_tx_provider_limitless_calls(
    n_calls: usize,
    input_chunks: Vec<Vec<InternalConsensusTransaction>>,
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

fn compare_tx_hashes(
    input: &[InternalConsensusTransaction],
    blockifier_input: &[BlockifierTransaction],
) -> bool {
    let expected_tx_hashes: Vec<TransactionHash> = input.iter().map(|tx| tx.tx_hash()).collect();
    let input_tx_hashes: Vec<TransactionHash> =
        blockifier_input.iter().map(BlockifierTransaction::tx_hash).collect();
    expected_tx_hashes == input_tx_hashes
}

async fn verify_build_block_output(
    expected_output_txs: Vec<InternalConsensusTransaction>,
    expected_block_artifacts: BlockExecutionArtifacts,
    result_block_artifacts: BlockExecutionArtifacts,
    mut output_stream_receiver: UnboundedReceiver<InternalConsensusTransaction>,
) {
    // Verify the transactions in the output channel.
    let mut output_txs = vec![];
    output_stream_receiver.recv_many(&mut output_txs, TX_CHANNEL_SIZE).await;

    assert_eq!(output_txs.len(), expected_output_txs.len());
    for tx in expected_output_txs.iter() {
        assert!(output_txs.contains(tx));
    }

    // Verify the block artifacts.
    assert_eq!(result_block_artifacts, expected_block_artifacts);
}

async fn run_build_block(
    mock_transaction_executor: MockTransactionExecutorTrait,
    tx_provider: MockTransactionProvider,
    output_sender: Option<UnboundedSender<InternalConsensusTransaction>>,
    fail_on_err: bool,
    abort_receiver: tokio::sync::oneshot::Receiver<()>,
    deadline_secs: u64,
) -> BlockBuilderResult<BlockExecutionArtifacts> {
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(deadline_secs);
    let transaction_converter = TransactionConverter::new(
        Arc::new(MockClassManagerClient::new()),
        ChainId::create_for_testing(),
    );
    let mut block_builder = BlockBuilder::new(
        Box::new(mock_transaction_executor),
        Box::new(tx_provider),
        output_sender,
        abort_receiver,
        transaction_converter,
        TX_CHUNK_SIZE,
        BlockBuilderExecutionParams { deadline, fail_on_err },
    );

    block_builder.build_block().await
}

#[rstest]
#[case::one_chunk_block(one_chunk_test_expectations())]
#[case::two_chunks_block(two_chunks_test_expectations())]
#[case::empty_block(empty_block_test_expectations())]
#[case::block_full(block_full_test_expectations())]
#[case::deadline_reached_after_first_chunk(test_expectations_with_delay())]
#[case::stream_done(stream_done_test_expectations())]
#[case::transaction_failed(transaction_failed_test_expectations())]
#[tokio::test]
async fn test_build_block(#[case] test_expectations: TestExpectations) {
    let (output_tx_sender, output_tx_receiver) = output_channel();
    let (_abort_sender, abort_receiver) = tokio::sync::oneshot::channel();

    let result_block_artifacts = run_build_block(
        test_expectations.mock_transaction_executor,
        test_expectations.mock_tx_provider,
        Some(output_tx_sender),
        false,
        abort_receiver,
        BLOCK_GENERATION_DEADLINE_SECS,
    )
    .await
    .unwrap();

    verify_build_block_output(
        test_expectations.expected_txs_output,
        test_expectations.expected_block_artifacts,
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

    let (_abort_sender, abort_receiver) = tokio::sync::oneshot::channel();
    let result_block_artifacts = run_build_block(
        mock_transaction_executor,
        mock_tx_provider,
        None,
        true,
        abort_receiver,
        BLOCK_GENERATION_DEADLINE_SECS,
    )
    .await
    .unwrap();

    assert_eq!(result_block_artifacts, expected_block_artifacts);
}

#[rstest]
#[case::block_full(test_txs(0..3), mock_transaction_executor_block_full(&input_txs), FailOnErrorCause::BlockFull)]
#[case::deadline_reached(test_txs(0..3), mock_transaction_executor_with_delay(&input_txs), FailOnErrorCause::DeadlineReached)]
#[tokio::test]
async fn test_validate_block_with_error(
    #[case] input_txs: Vec<InternalConsensusTransaction>,
    #[case] mut mock_transaction_executor: MockTransactionExecutorTrait,
    #[case] expected_error: FailOnErrorCause,
) {
    mock_transaction_executor.expect_close_block().times(0);

    let mock_tx_provider = mock_tx_provider_limited_calls(1, vec![input_txs]);

    let (_abort_sender, abort_receiver) = tokio::sync::oneshot::channel();
    let result = run_build_block(
        mock_transaction_executor,
        mock_tx_provider,
        None,
        true,
        abort_receiver,
        BLOCK_GENERATION_DEADLINE_SECS,
    )
    .await
    .unwrap_err();

    assert_matches!(
        result, BlockBuilderError::FailOnError(err)
        if err.to_string() == expected_error.to_string()
    );
}

#[rstest]
#[tokio::test]
async fn test_build_block_abort() {
    let mock_tx_provider = mock_tx_provider_limitless_calls(1, vec![test_txs(0..3)]);

    // Expect one transaction chunk to be added to the block, and then abort.
    let mut mock_transaction_executor = MockTransactionExecutorTrait::new();
    mock_transaction_executor
        .expect_add_txs_to_block()
        .return_once(|_| (0..3).map(|_| Ok(execution_info())).collect());
    mock_transaction_executor.expect_close_block().times(0);

    let (output_tx_sender, mut output_tx_receiver) = output_channel();
    let (abort_sender, abort_receiver) = tokio::sync::oneshot::channel();

    // Send the abort signal after the first tx is added to the block.
    tokio::spawn(async move {
        output_tx_receiver.recv().await.unwrap();
        abort_sender.send(()).unwrap();
    });

    assert_matches!(
        run_build_block(
            mock_transaction_executor,
            mock_tx_provider,
            Some(output_tx_sender),
            false,
            abort_receiver,
            BLOCK_GENERATION_LONG_DEADLINE_SECS,
        )
        .await,
        Err(BlockBuilderError::Aborted)
    );
}

#[rstest]
#[tokio::test]
async fn test_build_block_abort_immediately() {
    // Expect no transactions requested from the provider, and to be added to the block
    let mut mock_tx_provider = MockTransactionProvider::new();
    mock_tx_provider.expect_get_txs().times(0);
    let mut mock_transaction_executor = MockTransactionExecutorTrait::new();
    mock_transaction_executor.expect_add_txs_to_block().times(0);
    mock_transaction_executor.expect_close_block().times(0);

    let (output_tx_sender, _output_tx_receiver) = output_channel();
    let (abort_sender, abort_receiver) = tokio::sync::oneshot::channel();

    // Send the abort signal before we start building the block.
    abort_sender.send(()).unwrap();

    assert_matches!(
        run_build_block(
            mock_transaction_executor,
            mock_tx_provider,
            Some(output_tx_sender),
            false,
            abort_receiver,
            BLOCK_GENERATION_LONG_DEADLINE_SECS,
        )
        .await,
        Err(BlockBuilderError::Aborted)
    );
}

#[rstest]
#[tokio::test]
async fn test_l2_gas_used() {
    let n_txs = 3;
    let input_txs = test_txs(0..n_txs);
    let (mock_transaction_executor, _) = one_chunk_mock_executor(&input_txs, input_txs.len());
    let mock_tx_provider = mock_tx_provider_stream_done(input_txs);

    let (_abort_sender, abort_receiver) = tokio::sync::oneshot::channel();
    let result_block_artifacts = run_build_block(
        mock_transaction_executor,
        mock_tx_provider,
        None,
        true,
        abort_receiver,
        BLOCK_GENERATION_DEADLINE_SECS,
    )
    .await
    .unwrap();

    // Each mock transaction uses 1 L2 gas so the total amount should be the number of txs.
    assert_eq!(result_block_artifacts.l2_gas_used, GasAmount(n_txs.try_into().unwrap()));
}

// Test that the BlocBuilder returns the execution_infos ordered in the same order as
// the transactions are included in the block. This is crucial for the correct execution of
// starknet.
#[tokio::test]
async fn test_execution_info_order() {
    let input_txs = test_txs(0..6);
    let (mock_transaction_executor, _) = one_chunk_mock_executor(&input_txs, input_txs.len());
    let mock_tx_provider = mock_tx_provider_stream_done(input_txs.clone());
    let (_abort_sender, abort_receiver) = tokio::sync::oneshot::channel();

    let result_block_artifacts = run_build_block(
        mock_transaction_executor,
        mock_tx_provider,
        None,
        false,
        abort_receiver,
        BLOCK_GENERATION_DEADLINE_SECS,
    )
    .await
    .unwrap();

    // Verify that the execution_infos are ordered in the same order as the input_txs.
    result_block_artifacts.execution_infos.iter().zip(&input_txs).for_each(
        |((tx_hash, _execution_info), tx)| {
            assert_eq!(tx_hash, &tx.tx_hash());
        },
    );
}
