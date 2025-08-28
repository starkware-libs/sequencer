use std::sync::Arc;

use apollo_class_manager_types::transaction_converter::TransactionConverter;
use apollo_class_manager_types::MockClassManagerClient;
use apollo_l1_provider_types::InvalidValidationStatus;
use apollo_l1_provider_types::InvalidValidationStatus::{
    AlreadyIncludedInProposedBlock,
    AlreadyIncludedOnL2,
    ConsumedOnL1,
    NotFound,
};
use assert_matches::assert_matches;
use blockifier::blockifier::transaction_executor::{
    BlockExecutionSummary,
    TransactionExecutionOutput,
    TransactionExecutorError,
    TransactionExecutorResult,
};
use blockifier::bouncer::{BouncerWeights, CasmHashComputationData};
use blockifier::fee::fee_checks::FeeCheckError;
use blockifier::fee::receipt::TransactionReceipt;
use blockifier::state::cached_state::StateMaps;
use blockifier::state::errors::StateError;
use blockifier::transaction::objects::{RevertError, TransactionExecutionInfo};
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use indexmap::{IndexMap, IndexSet};
use itertools::chain;
use metrics_exporter_prometheus::PrometheusBuilder;
use mockall::predicate::eq;
use mockall::Sequence;
use pretty_assertions::assert_eq;
use rstest::rstest;
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::execution_resources::{GasAmount, GasVector};
use starknet_api::test_utils::CHAIN_ID_FOR_TESTS;
use starknet_api::transaction::fields::Fee;
use starknet_api::transaction::TransactionHash;
use starknet_api::tx_hash;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::block_builder::{
    BlockBuilder,
    BlockBuilderError,
    BlockBuilderExecutionParams,
    BlockBuilderResult,
    BlockBuilderTrait,
    BlockExecutionArtifacts,
    BlockTransactionExecutionData,
    FailOnErrorCause,
};
use crate::metrics::FULL_BLOCKS;
use crate::test_utils::{test_l1_handler_txs, test_txs};
use crate::transaction_executor::MockTransactionExecutorTrait;
use crate::transaction_provider::TransactionProviderError::L1HandlerTransactionValidationFailed;
use crate::transaction_provider::{MockTransactionProvider, TransactionProviderError};

const BLOCK_GENERATION_DEADLINE_SECS: u64 = 1;
const BLOCK_GENERATION_LONG_DEADLINE_SECS: u64 = 5;
const TX_CHANNEL_SIZE: usize = 50;
const N_CONCURRENT_TXS: usize = 3;
const TX_POLLING_INTERVAL: u64 = 100;

struct TestExpectations {
    mock_transaction_executor: MockTransactionExecutorTrait,
    mock_tx_provider: MockTransactionProvider,
    expected_block_artifacts: BlockExecutionArtifacts,
    expected_txs_output: Vec<InternalConsensusTransaction>,
    expected_full_blocks_metric: u64,
}

fn output_channel()
-> (UnboundedSender<InternalConsensusTransaction>, UnboundedReceiver<InternalConsensusTransaction>)
{
    tokio::sync::mpsc::unbounded_channel()
}

fn block_execution_artifacts(
    execution_infos: IndexMap<TransactionHash, TransactionExecutionInfo>,
    rejected_tx_hashes: IndexSet<TransactionHash>,
    consumed_l1_handler_tx_hashes: IndexSet<TransactionHash>,
    final_n_executed_txs: usize,
) -> BlockExecutionArtifacts {
    let l2_gas_used = GasAmount(execution_infos.len().try_into().unwrap());
    BlockExecutionArtifacts {
        execution_data: BlockTransactionExecutionData {
            execution_infos,
            rejected_tx_hashes,
            consumed_l1_handler_tx_hashes,
        },
        commitment_state_diff: Default::default(),
        compressed_state_diff: Default::default(),
        bouncer_weights: BouncerWeights { l1_gas: 100, ..BouncerWeights::empty() },
        // Each mock transaction uses 1 L2 gas so the total amount should be the number of txs.
        l2_gas_used,
        casm_hash_computation_data_sierra_gas: CasmHashComputationData::default(),
        casm_hash_computation_data_proving_gas: CasmHashComputationData::default(),
        compiled_class_hashes_for_migration: vec![],
        final_n_executed_txs,
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
        one_chunk_mock_executor(&input_txs, block_size, false);

    let mock_tx_provider = mock_tx_provider_limitless_calls(vec![input_txs.clone()]);

    TestExpectations {
        mock_transaction_executor,
        mock_tx_provider,
        expected_block_artifacts,
        expected_txs_output: input_txs,
        expected_full_blocks_metric: 0,
    }
}

struct ExpectationHelper {
    mock_transaction_executor: MockTransactionExecutorTrait,
    seq: Sequence,
}

impl ExpectationHelper {
    fn new() -> Self {
        Self {
            mock_transaction_executor: MockTransactionExecutorTrait::new(),
            seq: Sequence::new(),
        }
    }

    fn expect_add_txs_to_block(&mut self, input_txs: &[InternalConsensusTransaction]) {
        let input_txs_cloned = input_txs.to_vec();
        self.mock_transaction_executor
            .expect_add_txs_to_block()
            .times(1)
            .in_sequence(&mut self.seq)
            .withf(move |blockifier_input| compare_tx_hashes(&input_txs_cloned, blockifier_input))
            .return_const(());
    }

    fn expect_get_new_results_with_results(
        &mut self,
        results: Vec<TransactionExecutorResult<TransactionExecutionOutput>>,
    ) {
        self.mock_transaction_executor
            .expect_get_new_results()
            .times(1)
            .in_sequence(&mut self.seq)
            .return_once(move || results);
    }

    fn expect_successful_get_new_results(&mut self, n_txs: usize) {
        self.expect_get_new_results_with_results(
            (0..n_txs).map(|_| Ok((execution_info(), StateMaps::default()))).collect(),
        );
    }

    fn expect_is_done(&mut self, is_done: bool) {
        self.mock_transaction_executor
            .expect_is_done()
            .times(1)
            .in_sequence(&mut self.seq)
            .return_const(is_done);
    }

    /// Adds the expectations required for a block whose deadline is reached.
    /// For such a block, `get_new_results` and `is_done` will be called repeatedly until the
    /// deadline is reached.
    fn deadline_expectations(&mut self) {
        self.mock_transaction_executor.expect_get_new_results().returning(Vec::new);
        self.mock_transaction_executor.expect_is_done().return_const(false);
    }
}

fn one_chunk_mock_executor(
    input_txs: &[InternalConsensusTransaction],
    block_size: usize,
    is_validator: bool,
) -> (MockTransactionExecutorTrait, BlockExecutionArtifacts) {
    let mut helper = ExpectationHelper::new();

    helper.expect_successful_get_new_results(0);
    if !is_validator {
        helper.expect_is_done(false);
    }
    helper.expect_add_txs_to_block(input_txs);
    helper.expect_successful_get_new_results(block_size);
    if !is_validator {
        helper.expect_is_done(false);
    }
    helper.deadline_expectations();

    let expected_block_artifacts =
        set_close_block_expectations(&mut helper.mock_transaction_executor, block_size);
    (helper.mock_transaction_executor, expected_block_artifacts)
}

fn two_chunks_mock_executor(
    is_validator: bool,
) -> (
    Vec<InternalConsensusTransaction>,
    Vec<InternalConsensusTransaction>,
    MockTransactionExecutorTrait,
) {
    let input_txs = test_txs(0..6);
    let first_chunk = input_txs[..N_CONCURRENT_TXS].to_vec();
    let second_chunk = input_txs[N_CONCURRENT_TXS..].to_vec();

    let mut helper = ExpectationHelper::new();

    helper.expect_successful_get_new_results(0);
    if !is_validator {
        helper.expect_is_done(false);
    }
    helper.expect_add_txs_to_block(&first_chunk);
    helper.expect_successful_get_new_results(first_chunk.len());
    if !is_validator {
        helper.expect_is_done(false);
    }
    helper.expect_add_txs_to_block(&second_chunk);
    helper.expect_successful_get_new_results(second_chunk.len());
    if !is_validator {
        helper.expect_is_done(false);
    }
    helper.deadline_expectations();

    (first_chunk, second_chunk, helper.mock_transaction_executor)
}

fn two_chunks_test_expectations() -> TestExpectations {
    let (first_chunk, second_chunk, mut mock_transaction_executor) =
        two_chunks_mock_executor(false);
    let block_size = first_chunk.len() + second_chunk.len();

    let expected_block_artifacts =
        set_close_block_expectations(&mut mock_transaction_executor, block_size);

    let mock_tx_provider =
        mock_tx_provider_limitless_calls(vec![first_chunk.clone(), second_chunk.clone()]);

    TestExpectations {
        mock_transaction_executor,
        mock_tx_provider,
        expected_block_artifacts,
        expected_txs_output: chain!(first_chunk.iter(), second_chunk.iter()).cloned().collect(),
        expected_full_blocks_metric: 0,
    }
}

fn empty_block_test_expectations() -> TestExpectations {
    let mut helper = ExpectationHelper::new();
    helper.deadline_expectations();
    helper.mock_transaction_executor.expect_add_txs_to_block().times(0);

    let expected_block_artifacts =
        set_close_block_expectations(&mut helper.mock_transaction_executor, 0);

    let mock_tx_provider = mock_tx_provider_limitless_calls(vec![]);

    TestExpectations {
        mock_transaction_executor: helper.mock_transaction_executor,
        mock_tx_provider,
        expected_block_artifacts,
        expected_txs_output: vec![],
        expected_full_blocks_metric: 0,
    }
}

fn block_full_test_expectations(before_is_done: bool) -> TestExpectations {
    let input_txs = test_txs(0..3);

    let mut helper = ExpectationHelper::new();
    helper.expect_successful_get_new_results(0);
    helper.expect_is_done(false);
    helper.expect_add_txs_to_block(&input_txs);
    // Only the first transaction fits in the block.
    helper.expect_successful_get_new_results(if before_is_done { 1 } else { 0 });
    helper.expect_is_done(true);
    helper.expect_successful_get_new_results(if before_is_done { 0 } else { 1 });

    let mut mock_transaction_executor = helper.mock_transaction_executor;
    let expected_block_artifacts = set_close_block_expectations(&mut mock_transaction_executor, 1);

    let mock_tx_provider = mock_tx_provider_limited_calls(vec![input_txs.clone()]);

    TestExpectations {
        mock_transaction_executor,
        mock_tx_provider,
        expected_block_artifacts,
        expected_txs_output: input_txs,
        expected_full_blocks_metric: 1,
    }
}

fn mock_partial_transaction_execution(
    first_chunk: &[InternalConsensusTransaction],
    second_chunk: &[InternalConsensusTransaction],
    n_completed_txs: usize,
    is_validator: bool,
) -> MockTransactionExecutorTrait {
    assert!(n_completed_txs <= first_chunk.len());
    let mut helper = ExpectationHelper::new();
    helper.expect_successful_get_new_results(0);
    if !is_validator {
        helper.expect_is_done(false);
    }
    helper.expect_add_txs_to_block(first_chunk);
    if n_completed_txs > 0 {
        helper.expect_successful_get_new_results(n_completed_txs);
        if !is_validator {
            helper.expect_is_done(false);
        }
        helper.expect_add_txs_to_block(second_chunk);
    }

    // Do not return the results, simulating a deadline reached before the completion of the
    // transaction execution.
    helper.deadline_expectations();

    helper.mock_transaction_executor
}

fn test_expectations_partial_transaction_execution() -> TestExpectations {
    let n_completed_txs = 1;
    let input_txs = test_txs(0..N_CONCURRENT_TXS + n_completed_txs);
    let first_chunk = input_txs[0..N_CONCURRENT_TXS].to_vec();
    // After the execution of the first transaction, one more transaction is fetched from the
    // provider.
    let second_chunk = input_txs[N_CONCURRENT_TXS..].to_vec();
    let mut mock_transaction_executor =
        mock_partial_transaction_execution(&first_chunk, &second_chunk, n_completed_txs, false);

    let expected_block_artifacts =
        set_close_block_expectations(&mut mock_transaction_executor, n_completed_txs);

    let mock_tx_provider = mock_tx_provider_limited_calls(vec![first_chunk, second_chunk]);

    TestExpectations {
        mock_transaction_executor,
        mock_tx_provider,
        expected_block_artifacts,
        expected_txs_output: input_txs,
        expected_full_blocks_metric: 0,
    }
}

fn transaction_failed_test_expectations() -> TestExpectations {
    let n_txs = 6;
    let input_invoke_txs = test_txs(0..3);
    let input_l1_handler_txs = test_l1_handler_txs(3..n_txs);
    let failed_tx_indices = [1, 4];
    let failed_tx_hashes: IndexSet<TransactionHash> =
        failed_tx_indices.iter().map(|idx| tx_hash!(*idx)).collect();
    let consumed_l1_handler_tx_hashes: IndexSet<_> =
        input_l1_handler_txs.iter().map(|tx| tx.tx_hash()).collect();
    let input_txs: Vec<_> = input_invoke_txs.iter().chain(&input_l1_handler_txs).cloned().collect();

    let expected_txs_output: Vec<_> =
        input_txs.iter().filter(|tx| !failed_tx_hashes.contains(&tx.tx_hash())).cloned().collect();

    let mut helper = ExpectationHelper::new();
    helper.expect_successful_get_new_results(0);
    helper.expect_is_done(false);
    for start_idx in [0, 3] {
        helper.expect_add_txs_to_block(&input_txs[start_idx..start_idx + 3]);
        helper.expect_get_new_results_with_results(
            (start_idx..start_idx + 3)
                .map(|idx| {
                    if failed_tx_indices.contains(&idx) {
                        Err(TransactionExecutorError::StateError(
                            StateError::OutOfRangeContractAddress,
                        ))
                    } else {
                        Ok((execution_info(), StateMaps::default()))
                    }
                })
                .collect(),
        );
        helper.expect_is_done(false);
    }
    helper.deadline_expectations();

    let execution_infos_mapping =
        expected_txs_output.iter().map(|tx| (tx.tx_hash(), execution_info())).collect();

    let expected_block_artifacts = block_execution_artifacts(
        execution_infos_mapping,
        failed_tx_hashes,
        consumed_l1_handler_tx_hashes,
        n_txs,
    );
    let expected_block_artifacts_copy = expected_block_artifacts.clone();
    helper.mock_transaction_executor.expect_close_block().times(1).return_once(move |_| {
        Ok(BlockExecutionSummary {
            state_diff: expected_block_artifacts_copy.commitment_state_diff,
            compressed_state_diff: None,
            bouncer_weights: expected_block_artifacts_copy.bouncer_weights,
            casm_hash_computation_data_sierra_gas: expected_block_artifacts_copy
                .casm_hash_computation_data_sierra_gas,
            casm_hash_computation_data_proving_gas: expected_block_artifacts_copy
                .casm_hash_computation_data_proving_gas,
            compiled_class_hashes_for_migration: expected_block_artifacts_copy
                .compiled_class_hashes_for_migration,
        })
    });

    let mock_tx_provider =
        mock_tx_provider_limitless_calls(vec![input_invoke_txs, input_l1_handler_txs]);

    TestExpectations {
        mock_transaction_executor: helper.mock_transaction_executor,
        mock_tx_provider,
        expected_block_artifacts,
        expected_txs_output: input_txs,
        expected_full_blocks_metric: 0,
    }
}

// Fill the executor outputs with some non-default values to make sure the block_builder uses
// them.
fn block_builder_expected_output(
    execution_info_len: usize,
    final_n_executed_txs: usize,
) -> BlockExecutionArtifacts {
    let execution_info_len_u8 = u8::try_from(execution_info_len).unwrap();
    let execution_infos_mapping =
        (0..execution_info_len_u8).map(|i| (tx_hash!(i), execution_info())).collect();
    block_execution_artifacts(
        execution_infos_mapping,
        Default::default(),
        Default::default(),
        final_n_executed_txs,
    )
}

fn set_close_block_expectations(
    mock_transaction_executor: &mut MockTransactionExecutorTrait,
    block_size: usize,
) -> BlockExecutionArtifacts {
    let output_block_artifacts = block_builder_expected_output(block_size, block_size);
    let output_block_artifacts_copy = output_block_artifacts.clone();
    mock_transaction_executor.expect_close_block().times(1).return_once(move |_| {
        Ok(BlockExecutionSummary {
            state_diff: output_block_artifacts.commitment_state_diff,
            compressed_state_diff: None,
            bouncer_weights: output_block_artifacts.bouncer_weights,
            casm_hash_computation_data_sierra_gas: output_block_artifacts
                .casm_hash_computation_data_sierra_gas,
            casm_hash_computation_data_proving_gas: output_block_artifacts
                .casm_hash_computation_data_proving_gas,
            compiled_class_hashes_for_migration: output_block_artifacts
                .compiled_class_hashes_for_migration,
        })
    });
    output_block_artifacts_copy
}

/// Create a mock tx provider that will return the input chunks for number of chunks queries.
fn mock_tx_provider_limited_calls(
    input_chunks: Vec<Vec<InternalConsensusTransaction>>,
) -> MockTransactionProvider {
    mock_tx_provider_limited_calls_ex(input_chunks, None)
}

/// Create a mock tx provider that will return the input chunks for number of chunks queries.
fn mock_tx_provider_limited_calls_ex(
    input_chunks: Vec<Vec<InternalConsensusTransaction>>,
    final_n_executed_txs: Option<usize>,
) -> MockTransactionProvider {
    let mut mock_tx_provider = MockTransactionProvider::new();
    let mut seq = Sequence::new();
    for input_chunk in input_chunks {
        mock_tx_provider
            .expect_get_final_n_executed_txs()
            .times(1)
            .in_sequence(&mut seq)
            .return_const(None);
        mock_tx_provider
            .expect_get_txs()
            .times(1)
            .with(eq(input_chunk.len()))
            .in_sequence(&mut seq)
            .returning(move |_n_txs| Ok(input_chunk.clone()));
    }
    mock_tx_provider.expect_get_final_n_executed_txs().return_const(final_n_executed_txs);
    mock_tx_provider
}

fn mock_tx_provider_stream_done(
    input_chunk: Vec<InternalConsensusTransaction>,
) -> MockTransactionProvider {
    let n_txs = input_chunk.len();
    let mut mock_tx_provider = MockTransactionProvider::new();
    let mut seq = Sequence::new();
    mock_tx_provider
        .expect_get_final_n_executed_txs()
        .times(1)
        .in_sequence(&mut seq)
        .return_const(None);
    mock_tx_provider
        .expect_get_txs()
        .times(1)
        .in_sequence(&mut seq)
        .with(eq(N_CONCURRENT_TXS))
        .return_once(move |_n_txs| Ok(input_chunk));
    mock_tx_provider
        .expect_get_final_n_executed_txs()
        .times(1)
        .in_sequence(&mut seq)
        .return_const(Some(n_txs));

    // Continue to return empty chunks while the block is being built.
    mock_tx_provider.expect_get_txs().times(1..).returning(|_n_txs| Ok(vec![]));
    mock_tx_provider
}

/// Create a mock tx provider client that will return the input chunks and then empty chunks.
fn mock_tx_provider_limitless_calls(
    input_chunks: Vec<Vec<InternalConsensusTransaction>>,
) -> MockTransactionProvider {
    let mut mock_tx_provider = mock_tx_provider_limited_calls(input_chunks);

    // The number of times the mempool will be called until timeout is unpredicted.
    add_limitless_empty_calls(&mut mock_tx_provider);
    mock_tx_provider
}

fn add_limitless_empty_calls(mock_tx_provider: &mut MockTransactionProvider) {
    mock_tx_provider.expect_get_txs().with(eq(N_CONCURRENT_TXS)).returning(|_n_txs| Ok(Vec::new()));
    mock_tx_provider.expect_get_final_n_executed_txs().return_const(None);
}

/// Creates a `MockTransactionProvider` for less than (or exactly) N_CONCURRENT_TXS transactions.
fn mock_tx_provider_small_stream(
    input_chunk: Vec<InternalConsensusTransaction>,
) -> MockTransactionProvider {
    let mut mock_tx_provider = MockTransactionProvider::new();

    assert!(input_chunk.len() <= N_CONCURRENT_TXS);
    mock_tx_provider
        .expect_get_txs()
        .times(1)
        .with(eq(N_CONCURRENT_TXS))
        .returning(move |_n_txs| Ok(input_chunk.clone()));
    mock_tx_provider.expect_get_final_n_executed_txs().return_const(None);

    mock_tx_provider
}

fn mock_tx_provider_with_error(error: TransactionProviderError) -> MockTransactionProvider {
    let mut mock_tx_provider = MockTransactionProvider::new();
    mock_tx_provider
        .expect_get_txs()
        .times(1)
        .with(eq(N_CONCURRENT_TXS))
        .return_once(move |_n_txs| Err(error));
    mock_tx_provider.expect_get_final_n_executed_txs().return_const(None);
    mock_tx_provider
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

// TODO(yair): refactor to be a method of TestExpectations.
async fn verify_build_block_output(
    expected_output_txs: Vec<InternalConsensusTransaction>,
    expected_block_artifacts: BlockExecutionArtifacts,
    result_block_artifacts: BlockExecutionArtifacts,
    mut output_stream_receiver: UnboundedReceiver<InternalConsensusTransaction>,
    expected_full_blocks_metric: u64,
    metrics: &str,
) {
    // Verify the transactions in the output channel.
    let mut output_txs = vec![];
    output_stream_receiver.recv_many(&mut output_txs, TX_CHANNEL_SIZE).await;
    assert_eq!(output_txs, expected_output_txs);

    // Verify the block artifacts.
    assert_eq!(result_block_artifacts, expected_block_artifacts);

    FULL_BLOCKS.assert_eq::<u64>(metrics, expected_full_blocks_metric);
}

async fn run_build_block(
    mock_transaction_executor: MockTransactionExecutorTrait,
    tx_provider: MockTransactionProvider,
    output_sender: Option<UnboundedSender<InternalConsensusTransaction>>,
    is_validator: bool,
    abort_receiver: tokio::sync::oneshot::Receiver<()>,
    deadline_secs: u64,
) -> BlockBuilderResult<BlockExecutionArtifacts> {
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(deadline_secs);
    let transaction_converter = TransactionConverter::new(
        Arc::new(MockClassManagerClient::new()),
        CHAIN_ID_FOR_TESTS.clone(),
    );
    let mut block_builder = BlockBuilder::new(
        mock_transaction_executor,
        Box::new(tx_provider),
        output_sender,
        None,
        None,
        abort_receiver,
        transaction_converter,
        N_CONCURRENT_TXS,
        TX_POLLING_INTERVAL,
        BlockBuilderExecutionParams { deadline, is_validator },
    );

    block_builder.build_block().await
}

#[rstest]
#[case::one_chunk_block(one_chunk_test_expectations())]
#[case::two_chunks_block(two_chunks_test_expectations())]
#[case::empty_block(empty_block_test_expectations())]
#[case::block_full_before_is_done(block_full_test_expectations(true))]
#[case::block_full_after_is_done(block_full_test_expectations(false))]
#[case::deadline_reached_after_first_chunk(test_expectations_partial_transaction_execution())]
#[case::transaction_failed(transaction_failed_test_expectations())]
#[tokio::test]
async fn test_build_block(#[case] test_expectations: TestExpectations) {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    FULL_BLOCKS.register();
    let metrics = recorder.handle().render();
    FULL_BLOCKS.assert_eq::<u64>(&metrics, 0);

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
        test_expectations.expected_full_blocks_metric,
        &recorder.handle().render(),
    )
    .await;
}

#[tokio::test]
async fn test_validate_block() {
    let input_txs = test_txs(0..3);
    let (mock_transaction_executor, expected_block_artifacts) =
        one_chunk_mock_executor(&input_txs, input_txs.len(), true);
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

/// Tests the case where the final number of transactions in the block is smaller than the number
/// of transactions that were executed.
#[tokio::test]
async fn test_validate_block_excluded_txs() {
    let (first_chunk, second_chunk, mut mock_transaction_executor) = two_chunks_mock_executor(true);
    let n_executed_txs = first_chunk.len() + second_chunk.len();
    let final_n_executed_txs = n_executed_txs - 1;

    let expected_block_artifacts =
        set_close_block_expectations(&mut mock_transaction_executor, final_n_executed_txs);

    let mut mock_tx_provider = mock_tx_provider_limited_calls_ex(
        vec![first_chunk, second_chunk],
        Some(final_n_executed_txs),
    );

    mock_tx_provider.expect_get_txs().returning(move |_n_txs| Ok(vec![]));

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
#[case::deadline_reached(
    test_txs(0..3), mock_partial_transaction_execution(&input_txs, &[], 0, true),
    FailOnErrorCause::DeadlineReached
)]
#[tokio::test]
async fn test_validate_block_with_error(
    #[case] input_txs: Vec<InternalConsensusTransaction>,
    #[case] mut mock_transaction_executor: MockTransactionExecutorTrait,
    #[case] expected_error: FailOnErrorCause,
) {
    mock_transaction_executor.expect_close_block().times(0);
    mock_transaction_executor.expect_abort_block().times(1).return_once(|| ());

    let mock_tx_provider = mock_tx_provider_limited_calls(vec![input_txs]);

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
#[case::already_included_in_proposed_block(AlreadyIncludedInProposedBlock)]
#[case::already_included_on_l2(AlreadyIncludedOnL2)]
#[case::consumed_on_l1(ConsumedOnL1)]
#[case::not_found(NotFound)]
#[tokio::test]
async fn test_validate_block_l1_handler_validation_error(#[case] status: InvalidValidationStatus) {
    let tx_provider = mock_tx_provider_with_error(L1HandlerTransactionValidationFailed {
        tx_hash: tx_hash!(0),
        validation_status: status,
    });

    let (_abort_sender, abort_receiver) = tokio::sync::oneshot::channel();

    let mut helper = ExpectationHelper::new();
    helper.deadline_expectations();

    helper.mock_transaction_executor.expect_abort_block().times(1).return_once(|| ());

    let result = run_build_block(
        helper.mock_transaction_executor,
        tx_provider,
        None,
        true,
        abort_receiver,
        BLOCK_GENERATION_DEADLINE_SECS,
    )
    .await;

    assert_matches!(
        result,
        Err(BlockBuilderError::FailOnError(
            FailOnErrorCause::L1HandlerTransactionValidationFailed(
                TransactionProviderError::L1HandlerTransactionValidationFailed { .. }
            )
        )),
        "Expected FailOnError for validation status: {status:?}"
    );
}

#[rstest]
#[tokio::test]
async fn test_build_block_abort() {
    let n_txs = 3;
    let mock_tx_provider = mock_tx_provider_limitless_calls(vec![test_txs(0..n_txs)]);

    // Expect one transaction chunk to be added to the block, and then abort.
    let mut helper = ExpectationHelper::new();
    helper.expect_successful_get_new_results(0);
    helper.expect_is_done(false);
    helper.expect_add_txs_to_block(&test_txs(0..3));
    helper.expect_successful_get_new_results(3);
    helper.expect_is_done(false);
    helper.deadline_expectations();

    helper.mock_transaction_executor.expect_close_block().times(0);
    helper.mock_transaction_executor.expect_abort_block().times(1).return_once(|| ());

    let (output_tx_sender, mut output_tx_receiver) = output_channel();
    let (abort_sender, abort_receiver) = tokio::sync::oneshot::channel();

    // Send the abort signal after the first tx is added to the block.
    tokio::spawn(async move {
        output_tx_receiver.recv().await.unwrap();
        abort_sender.send(()).unwrap();
    });

    assert_matches!(
        run_build_block(
            helper.mock_transaction_executor,
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
    mock_tx_provider.expect_get_final_n_executed_txs().return_const(None);
    let mut mock_transaction_executor = MockTransactionExecutorTrait::new();
    mock_transaction_executor.expect_add_txs_to_block().times(0);
    mock_transaction_executor.expect_close_block().times(0);
    mock_transaction_executor.expect_abort_block().times(1).return_once(|| ());

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
    let (mock_transaction_executor, _) = one_chunk_mock_executor(&input_txs, input_txs.len(), true);
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
    let (first_chunk, second_chunk, mut mock_transaction_executor) =
        two_chunks_mock_executor(false);
    let input_txs = first_chunk.iter().chain(second_chunk.iter()).collect::<Vec<_>>();

    set_close_block_expectations(&mut mock_transaction_executor, input_txs.len());

    let mock_tx_provider =
        mock_tx_provider_limitless_calls(vec![first_chunk.clone(), second_chunk.clone()]);
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
    result_block_artifacts.execution_data.execution_infos.iter().zip(&input_txs).for_each(
        |((tx_hash, _execution_info), tx)| {
            assert_eq!(tx_hash, &tx.tx_hash());
        },
    );
}

#[rstest]
#[tokio::test]
async fn failed_l1_handler_transaction_consumed() {
    let l1_handler_txs = test_l1_handler_txs(0..2);
    let mock_tx_provider = mock_tx_provider_small_stream(l1_handler_txs.clone());

    let mut helper = ExpectationHelper::new();
    helper.expect_successful_get_new_results(0);
    helper.expect_is_done(false);
    helper.expect_add_txs_to_block(&l1_handler_txs);
    helper.expect_get_new_results_with_results(vec![
        Err(TransactionExecutorError::StateError(StateError::OutOfRangeContractAddress)),
        Ok((execution_info(), StateMaps::default())),
    ]);
    helper.expect_is_done(true);
    helper.expect_successful_get_new_results(0);

    helper.mock_transaction_executor.expect_close_block().times(1).return_once(|_| {
        Ok(BlockExecutionSummary {
            state_diff: Default::default(),
            compressed_state_diff: None,
            bouncer_weights: BouncerWeights::empty(),
            casm_hash_computation_data_sierra_gas: CasmHashComputationData::default(),
            casm_hash_computation_data_proving_gas: CasmHashComputationData::default(),
            compiled_class_hashes_for_migration: vec![],
        })
    });

    let (_abort_sender, abort_receiver) = tokio::sync::oneshot::channel();
    let result_block_artifacts = run_build_block(
        helper.mock_transaction_executor,
        mock_tx_provider,
        None,
        false,
        abort_receiver,
        BLOCK_GENERATION_DEADLINE_SECS,
    )
    .await
    .unwrap();

    // Verify that all L1 handler transaction's are included in the consumed l1 transactions.
    assert_eq!(
        result_block_artifacts.execution_data.consumed_l1_handler_tx_hashes,
        l1_handler_txs.iter().map(|tx| tx.tx_hash()).collect::<IndexSet<_>>()
    );
}

#[tokio::test]
async fn partial_chunk_execution_proposer() {
    let input_txs = test_txs(0..3); // Assume 3 TXs were sent.
    let executed_txs = input_txs[..2].to_vec(); // Only 2 should be processed. Simulating a partial chunk execution.

    let expected_execution_infos: IndexMap<_, _> =
        executed_txs.iter().map(|tx| (tx.tx_hash(), execution_info())).collect();

    let mut helper = ExpectationHelper::new();

    helper.expect_successful_get_new_results(0);
    helper.expect_is_done(false);
    helper.expect_add_txs_to_block(&input_txs);
    // Return only 2 txs, simulating a partial chunk execution.
    helper.expect_successful_get_new_results(executed_txs.len());
    helper.expect_is_done(true);
    helper.expect_successful_get_new_results(0);

    let expected_block_artifacts = block_execution_artifacts(
        expected_execution_infos,
        Default::default(),
        Default::default(),
        executed_txs.len(),
    );

    let expected_block_artifacts_copy = expected_block_artifacts.clone();
    helper.mock_transaction_executor.expect_close_block().times(1).return_once(move |_| {
        Ok(BlockExecutionSummary {
            state_diff: expected_block_artifacts.commitment_state_diff,
            compressed_state_diff: None,
            bouncer_weights: expected_block_artifacts.bouncer_weights,
            casm_hash_computation_data_sierra_gas: expected_block_artifacts
                .casm_hash_computation_data_sierra_gas,
            casm_hash_computation_data_proving_gas: expected_block_artifacts
                .casm_hash_computation_data_proving_gas,
            compiled_class_hashes_for_migration: expected_block_artifacts
                .compiled_class_hashes_for_migration,
        })
    });

    let mock_tx_provider = mock_tx_provider_limited_calls(vec![input_txs.clone()]);
    let (_abort_sender, abort_receiver) = tokio::sync::oneshot::channel();

    // Block should be built with the executed transactions without any errors.
    let is_validator = false;
    let result_block_artifacts = run_build_block(
        helper.mock_transaction_executor,
        mock_tx_provider,
        None,
        is_validator,
        abort_receiver,
        BLOCK_GENERATION_DEADLINE_SECS,
    )
    .await
    .unwrap();

    assert_eq!(result_block_artifacts, expected_block_artifacts_copy);
}

#[rstest]
#[case::success(true)]
#[case::fail(false)]
#[tokio::test]
async fn partial_chunk_execution_validator(#[case] successful: bool) {
    let input_txs = test_txs(0..3);

    let mut helper = ExpectationHelper::new();
    helper.expect_successful_get_new_results(0);
    helper.expect_add_txs_to_block(&input_txs);
    // Return only 2 txs, simulating a partial chunk execution.
    helper.expect_successful_get_new_results(2);

    let expected_block_artifacts = if successful {
        helper.mock_transaction_executor.expect_abort_block().times(0);
        Some(set_close_block_expectations(&mut helper.mock_transaction_executor, 2))
    } else {
        // Validator continues the loop even after the scheduler is done.
        helper.mock_transaction_executor.expect_get_new_results().times(1..).returning(Vec::new);

        helper.mock_transaction_executor.expect_close_block().times(0);
        helper.mock_transaction_executor.expect_abort_block().times(1).return_once(|| ());
        None
    };

    // Success: the proposer suggests final_n_executed_txs=2, and since those were executed
    // successfully, the validator succeeds.
    // Fail: the proposer suggests final_n_executed_txs=3, and the validator fails.
    let final_n_executed_txs = if successful { 2 } else { 3 };
    let mut mock_tx_provider =
        mock_tx_provider_limited_calls_ex(vec![input_txs.clone()], Some(final_n_executed_txs));
    mock_tx_provider.expect_get_txs().with(eq(2)).returning(|_n_txs| Ok(Vec::new()));

    let (_abort_sender, abort_receiver) = tokio::sync::oneshot::channel();

    let is_validator = true;
    let result_block_artifacts = run_build_block(
        helper.mock_transaction_executor,
        mock_tx_provider,
        None,
        is_validator,
        abort_receiver,
        BLOCK_GENERATION_DEADLINE_SECS,
    )
    .await;

    if successful {
        assert_eq!(result_block_artifacts.unwrap(), expected_block_artifacts.unwrap());
    } else {
        // Deadline is reached since the validator never completes 3 transactions.
        assert!(matches!(
            result_block_artifacts,
            Err(BlockBuilderError::FailOnError(FailOnErrorCause::DeadlineReached))
        ));
    }
}
