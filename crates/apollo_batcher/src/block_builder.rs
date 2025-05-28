use std::cmp::min;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use apollo_batcher_types::batcher_types::ProposalCommitment;
use apollo_class_manager_types::transaction_converter::{
    TransactionConverter,
    TransactionConverterError,
    TransactionConverterResult,
    TransactionConverterTrait,
};
use apollo_class_manager_types::SharedClassManagerClient;
use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_infra_utils::tracing::LogCompatibleToStringExt;
use apollo_state_reader::papyrus_state::{ClassReader, PapyrusReader};
use apollo_storage::StorageReader;
use async_trait::async_trait;
use blockifier::blockifier::concurrent_transaction_executor::ConcurrentTransactionExecutor;
use blockifier::blockifier::config::WorkerPoolConfig;
use blockifier::blockifier::transaction_executor::{
    BlockExecutionSummary,
    TransactionExecutionOutput,
    TransactionExecutorError as BlockifierTransactionExecutorError,
    TransactionExecutorResult,
};
use blockifier::blockifier_versioned_constants::{VersionedConstants, VersionedConstantsOverrides};
use blockifier::bouncer::{BouncerConfig, BouncerWeights, CasmHashComputationData};
use blockifier::concurrency::worker_pool::WorkerPool;
use blockifier::context::{BlockContext, ChainInfo};
use blockifier::state::cached_state::{CachedState, CommitmentStateDiff};
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier::state::errors::StateError;
use blockifier::state::state_reader_and_contract_manager::StateReaderAndContractManager;
use blockifier::transaction::objects::TransactionExecutionInfo;
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use indexmap::{IndexMap, IndexSet};
#[cfg(test)]
use mockall::automock;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHashAndNumber, BlockInfo};
use starknet_api::block_hash::state_diff_hash::calculate_state_diff_hash;
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::execution_resources::GasAmount;
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::TransactionHash;
use thiserror::Error;
use tokio::sync::{Mutex, MutexGuard};
use tracing::{debug, error, info, trace, warn};

use crate::block_builder::FailOnErrorCause::L1HandlerTransactionValidationFailed;
use crate::cende_client_types::{StarknetClientStateDiff, StarknetClientTransactionReceipt};
use crate::metrics::FULL_BLOCKS;
use crate::pre_confirmed_block_writer::{CandidateTxSender, PreconfirmedTxSender};
use crate::transaction_executor::TransactionExecutorTrait;
use crate::transaction_provider::{TransactionProvider, TransactionProviderError};

#[derive(Debug, Error)]
pub enum BlockBuilderError {
    #[error(transparent)]
    BlockifierStateError(#[from] StateError),
    #[error(transparent)]
    ExecutorError(#[from] BlockifierTransactionExecutorError),
    #[error(transparent)]
    GetTransactionError(#[from] TransactionProviderError),
    #[error(transparent)]
    StreamTransactionsError(
        #[from] tokio::sync::mpsc::error::SendError<InternalConsensusTransaction>,
    ),
    #[error(transparent)]
    FailOnError(FailOnErrorCause),
    #[error("The block builder was aborted.")]
    Aborted,
    #[error(transparent)]
    TransactionConverterError(#[from] TransactionConverterError),
}

pub type BlockBuilderResult<T> = Result<T, BlockBuilderError>;

#[derive(Debug, Error)]
pub enum FailOnErrorCause {
    #[error("Block is full")]
    BlockFull,
    #[error("Deadline has been reached")]
    DeadlineReached,
    #[error("Transaction failed: {0}")]
    TransactionFailed(BlockifierTransactionExecutorError),
    #[error("L1 Handler transaction validation failed")]
    L1HandlerTransactionValidationFailed(TransactionProviderError),
}

enum AddTxsToExecutorResult {
    NoNewTxs,
    NewTxs,
}

#[cfg_attr(test, derive(Clone))]
#[derive(Debug, PartialEq)]
pub struct BlockExecutionArtifacts {
    // Note: The execution_infos must be ordered to match the order of the transactions in the
    // block.
    pub execution_data: BlockTransactionExecutionData,
    pub commitment_state_diff: CommitmentStateDiff,
    pub compressed_state_diff: Option<CommitmentStateDiff>,
    pub bouncer_weights: BouncerWeights,
    pub l2_gas_used: GasAmount,
    pub casm_hash_computation_data_sierra_gas: CasmHashComputationData,
    pub casm_hash_computation_data_proving_gas: CasmHashComputationData,
    // The number of transactions executed by the proposer out of the transactions that were sent.
    // This value includes rejected transactions.
    pub final_n_executed_txs: usize,
}

impl BlockExecutionArtifacts {
    pub fn address_to_nonce(&self) -> HashMap<ContractAddress, Nonce> {
        HashMap::from_iter(
            self.commitment_state_diff
                .address_to_nonce
                .iter()
                .map(|(address, nonce)| (*address, *nonce)),
        )
    }

    pub fn tx_hashes(&self) -> HashSet<TransactionHash> {
        HashSet::from_iter(self.execution_data.execution_infos.keys().copied())
    }

    pub fn thin_state_diff(&self) -> ThinStateDiff {
        // TODO(Ayelet): Remove the clones.
        let commitment_state_diff = self.commitment_state_diff.clone();
        ThinStateDiff {
            deployed_contracts: commitment_state_diff.address_to_class_hash,
            storage_diffs: commitment_state_diff.storage_updates,
            declared_classes: commitment_state_diff.class_hash_to_compiled_class_hash,
            nonces: commitment_state_diff.address_to_nonce,
            // TODO(AlonH): Remove this when the structure of storage diffs changes.
            deprecated_declared_classes: Vec::new(),
        }
    }

    pub fn commitment(&self) -> ProposalCommitment {
        ProposalCommitment {
            state_diff_commitment: calculate_state_diff_hash(&self.thin_state_diff()),
        }
    }
}

/// The BlockBuilderTrait is responsible for building a new block from transactions provided by the
/// tx_provider. The block building will stop at time deadline.
/// The transactions that were added to the block will be streamed to the output_content_sender.
#[cfg_attr(test, automock)]
#[async_trait]
pub trait BlockBuilderTrait: Send {
    async fn build_block(&mut self) -> BlockBuilderResult<BlockExecutionArtifacts>;
}

pub struct BlockBuilderExecutionParams {
    pub deadline: tokio::time::Instant,
    pub is_validator: bool,
}

pub struct BlockBuilder {
    // TODO(Yael 14/10/2024): make the executor thread safe and delete this mutex.
    executor: Arc<Mutex<dyn TransactionExecutorTrait>>,
    tx_provider: Box<dyn TransactionProvider>,
    output_content_sender: Option<tokio::sync::mpsc::UnboundedSender<InternalConsensusTransaction>>,
    /// The senders are utilized only during block proposal and not during block validation.
    candidate_tx_sender: Option<CandidateTxSender>,
    pre_confirmed_tx_sender: Option<PreconfirmedTxSender>,
    abort_signal_receiver: tokio::sync::oneshot::Receiver<()>,
    transaction_converter: TransactionConverter,
    /// The number of transactions whose execution is completed.
    n_executed_txs: usize,
    /// The transactions whose execution started.
    block_txs: Vec<InternalConsensusTransaction>,
    execution_data: BlockTransactionExecutionData,

    /// Parameters to configure the block builder behavior.
    n_concurrent_txs: usize,
    tx_polling_interval_millis: u64,
    execution_params: BlockBuilderExecutionParams,
}

impl BlockBuilder {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        executor: impl TransactionExecutorTrait + 'static,
        tx_provider: Box<dyn TransactionProvider>,
        output_content_sender: Option<
            tokio::sync::mpsc::UnboundedSender<InternalConsensusTransaction>,
        >,
        candidate_tx_sender: Option<CandidateTxSender>,
        pre_confirmed_tx_sender: Option<PreconfirmedTxSender>,
        abort_signal_receiver: tokio::sync::oneshot::Receiver<()>,
        transaction_converter: TransactionConverter,
        n_concurrent_txs: usize,
        tx_polling_interval_millis: u64,
        execution_params: BlockBuilderExecutionParams,
    ) -> Self {
        let executor = Arc::new(Mutex::new(executor));
        Self {
            executor,
            tx_provider,
            output_content_sender,
            candidate_tx_sender,
            pre_confirmed_tx_sender,
            abort_signal_receiver,
            transaction_converter,
            n_executed_txs: 0,
            block_txs: Vec::new(),
            execution_data: BlockTransactionExecutionData::default(),
            n_concurrent_txs,
            tx_polling_interval_millis,
            execution_params,
        }
    }
}

#[async_trait]
impl BlockBuilderTrait for BlockBuilder {
    async fn build_block(&mut self) -> BlockBuilderResult<BlockExecutionArtifacts> {
        let res = self.build_block_inner().await;
        if res.is_err() {
            self.executor.lock().await.abort_block();
        }
        res
    }
}

impl BlockBuilder {
    async fn build_block_inner(&mut self) -> BlockBuilderResult<BlockExecutionArtifacts> {
        let mut final_n_executed_txs: Option<usize> = None;
        while !self.finished_block_txs(final_n_executed_txs) {
            if tokio::time::Instant::now() >= self.execution_params.deadline {
                info!("Block builder deadline reached.");
                if self.execution_params.is_validator {
                    return Err(BlockBuilderError::FailOnError(FailOnErrorCause::DeadlineReached));
                }
                break;
            }
            if final_n_executed_txs.is_none() {
                if let Some(res) = self.tx_provider.get_final_n_executed_txs().await {
                    info!("Received final number of transactions in block proposal: {res}.");
                    final_n_executed_txs = Some(res);
                }
            }
            if self.abort_signal_receiver.try_recv().is_ok() {
                info!("Received abort signal. Aborting block builder.");
                return Err(BlockBuilderError::Aborted);
            }

            self.handle_executed_txs().await?;

            // Check if the block is full. This is only relevant in propose mode.
            // In validate mode, this is ignored and we simply wait for the proposer to send the
            // final number of transactions in the block.
            if !self.execution_params.is_validator && lock_executor(&self.executor).is_done() {
                // Call `handle_executed_txs()` once more to get the last results.
                self.handle_executed_txs().await?;
                info!("Block is full.");
                FULL_BLOCKS.increment(1);
                break;
            }

            match self.add_txs_to_executor().await? {
                AddTxsToExecutorResult::NoNewTxs => self.sleep().await,
                AddTxsToExecutorResult::NewTxs => {}
            }
        }

        // The final number of transactions to consider for the block.
        // Proposer: this is the number of transactions that were executed.
        // Validator: the number of transactions we got from the proposer.
        let final_n_executed_txs_nonopt = if self.execution_params.is_validator {
            final_n_executed_txs.expect("final_n_executed_txs must be set in validate mode.")
        } else {
            assert!(
                final_n_executed_txs.is_none(),
                "final_n_executed_txs must be None in propose mode."
            );
            self.n_executed_txs
        };

        info!(
            "Finished building block. Started executing {} transactions. Finished executing {} \
             transactions. Final number of transactions (as set by the proposer): {}.",
            self.block_txs.len(),
            self.n_executed_txs,
            final_n_executed_txs_nonopt,
        );

        // Move a clone of the executor into the lambda function.
        let executor = self.executor.clone();
        let block_summary = tokio::task::spawn_blocking(move || {
            lock_executor(&executor).close_block(final_n_executed_txs_nonopt)
        })
        .await
        .expect("Failed to spawn blocking executor task.")?;

        let BlockExecutionSummary {
            state_diff,
            compressed_state_diff,
            bouncer_weights,
            casm_hash_computation_data_sierra_gas,
            casm_hash_computation_data_proving_gas,
        } = block_summary;
        let mut execution_data = std::mem::take(&mut self.execution_data);
        if let Some(final_n_executed_txs) = final_n_executed_txs {
            // Remove the transactions that were executed, but eventually not included in the block.
            // This can happen if the proposer sends some transactions but closes the block before
            // including them, while the validator already executed those transactions.
            let remove_tx_hashes: Vec<TransactionHash> =
                self.block_txs[final_n_executed_txs..].iter().map(|tx| tx.tx_hash()).collect();
            execution_data.remove_last_txs(&remove_tx_hashes);
        }
        let l2_gas_used = execution_data.l2_gas_used();
        Ok(BlockExecutionArtifacts {
            execution_data,
            commitment_state_diff: state_diff,
            compressed_state_diff,
            bouncer_weights,
            l2_gas_used,
            casm_hash_computation_data_sierra_gas,
            casm_hash_computation_data_proving_gas,
            final_n_executed_txs: final_n_executed_txs_nonopt,
        })
    }

    /// Returns the number of transactions that are currently being executed by the executor.
    fn n_txs_in_progress(&self) -> usize {
        self.block_txs.len() - self.n_executed_txs
    }

    /// Returns `true` if all the txs in the block were executed. This function always returns
    /// `false` in propose mode.
    fn finished_block_txs(&self, final_n_executed_txs: Option<usize>) -> bool {
        if let Some(final_n_executed_txs) = final_n_executed_txs {
            self.n_executed_txs >= final_n_executed_txs
        } else {
            // final_n_executed_txs is not known yet, so the block is not finished.
            false
        }
    }

    /// Adds new transactions (if there are any) from `tx_provider` to the executor.
    ///
    /// Returns whether new transactions were added and whether the transaction stream is exhausted
    /// (this can only happen in validator mode).
    async fn add_txs_to_executor(&mut self) -> BlockBuilderResult<AddTxsToExecutorResult> {
        // Restrict the number of transactions to fetch such that the number of transactions in
        // progress is at most `n_concurrent_txs`.
        let n_txs_to_fetch =
            self.n_concurrent_txs - min(self.n_txs_in_progress(), self.n_concurrent_txs);

        if n_txs_to_fetch == 0 {
            return Ok(AddTxsToExecutorResult::NoNewTxs);
        }

        let next_txs = match self.tx_provider.get_txs(n_txs_to_fetch).await {
            Err(e @ TransactionProviderError::L1HandlerTransactionValidationFailed { .. })
                if self.execution_params.is_validator =>
            {
                return Err(BlockBuilderError::FailOnError(L1HandlerTransactionValidationFailed(
                    e,
                )));
            }
            Err(err) => {
                error!("Failed to get transactions from the transaction provider: {:?}", err);
                return Err(err.into());
            }
            Ok(result) => result,
        };

        let n_txs = next_txs.len();
        debug!("Got {} transactions from the transaction provider.", n_txs);
        if next_txs.is_empty() {
            return Ok(AddTxsToExecutorResult::NoNewTxs);
        }

        self.send_candidate_txs(&next_txs);

        self.block_txs.extend(next_txs.iter().cloned());

        let tx_convert_futures = next_txs.iter().map(|tx| async {
            convert_to_executable_blockifier_tx(&self.transaction_converter, tx.clone()).await
        });
        let executor_input_chunk = futures::future::try_join_all(tx_convert_futures).await?;

        // Start the execution of the transactions on the worker pool.
        info!("Starting execution of {} transactions.", n_txs);
        lock_executor(&self.executor).add_txs_to_block(executor_input_chunk.as_slice());

        if let Some(output_content_sender) = &self.output_content_sender {
            // Send the transactions to the validators.
            // Only reached in proposal flow.
            for tx in next_txs.into_iter() {
                output_content_sender.send(tx)?;
            }
        }

        Ok(AddTxsToExecutorResult::NewTxs)
    }

    /// Handles the transactions that were executed so far by the executor.
    async fn handle_executed_txs(&mut self) -> BlockBuilderResult<()> {
        let results = lock_executor(&self.executor).get_new_results();

        if results.is_empty() {
            return Ok(());
        }

        info!("Finished execution of {} transactions.", results.len());

        let old_n_executed_txs = self.n_executed_txs;
        self.n_executed_txs += results.len();

        collect_execution_results_and_stream_txs(
            &self.block_txs[old_n_executed_txs..self.n_executed_txs],
            results,
            &mut self.execution_data,
            &self.pre_confirmed_tx_sender,
        )
        .await
    }

    fn send_candidate_txs(&mut self, next_tx_chunk: &[InternalConsensusTransaction]) {
        // Skip sending candidate transactions during validation flow.
        // In validate flow candidate_tx_sender is None.
        let Some(candidate_tx_sender) = &self.candidate_tx_sender else {
            return;
        };

        let txs = next_tx_chunk.to_vec();
        let num_txs = txs.len();

        trace!(
            "Attempting to send a candidate transaction chunk with {num_txs} transactions to the \
             PreconfirmedBlockWriter.",
        );

        match candidate_tx_sender.try_send(txs) {
            Ok(_) => {
                info!(
                    "Successfully sent a candidate transaction chunk with {num_txs} transactions \
                     to the PreconfirmedBlockWriter.",
                );
            }
            // We continue with block building even if sending candidate transactions to
            // the PreconfirmedBlockWriter fails because it is not critical for the block
            // building process.
            Err(err) => {
                error!(
                    "Failed to send a candidate transaction chunk with {num_txs} transactions to \
                     the PreconfirmedBlockWriter: {:?}",
                    err
                );
            }
        }
    }

    async fn sleep(&mut self) {
        tokio::time::sleep(tokio::time::Duration::from_millis(self.tx_polling_interval_millis))
            .await;
    }
}

fn lock_executor<'a>(
    executor: &'a Arc<Mutex<dyn TransactionExecutorTrait>>,
) -> MutexGuard<'a, dyn TransactionExecutorTrait> {
    executor.try_lock().expect("Only a single task should use the executor.")
}

async fn convert_to_executable_blockifier_tx(
    transaction_converter: &TransactionConverter,
    tx: InternalConsensusTransaction,
) -> TransactionConverterResult<BlockifierTransaction> {
    let executable_tx =
        transaction_converter.convert_internal_consensus_tx_to_executable_tx(tx).await?;
    Ok(BlockifierTransaction::new_for_sequencing(executable_tx))
}

async fn collect_execution_results_and_stream_txs(
    tx_chunk: &[InternalConsensusTransaction],
    results: Vec<TransactionExecutorResult<TransactionExecutionOutput>>,
    execution_data: &mut BlockTransactionExecutionData,
    pre_confirmed_tx_sender: &Option<PreconfirmedTxSender>,
) -> BlockBuilderResult<()> {
    assert!(
        results.len() == tx_chunk.len(),
        "The number of results match the number of transactions."
    );

    for (input_tx, result) in tx_chunk.iter().zip(results.into_iter()) {
        let optional_l1_handler_tx =
            if let InternalConsensusTransaction::L1Handler(l1_handler_tx) = input_tx {
                Some(l1_handler_tx.tx.clone())
            } else {
                None
            };
        let tx_hash = input_tx.tx_hash();

        // Insert the tx_hash into the appropriate collection if it's an L1_Handler transaction.
        if let InternalConsensusTransaction::L1Handler(_) = input_tx {
            let is_new_entry = execution_data.consumed_l1_handler_tx_hashes.insert(tx_hash);
            // Even though this doesn't get past the set insertion, this indicates a major, possibly
            // reorg-producing bug, either in some batcher cache or the l1 provider.
            assert!(is_new_entry, "Duplicate L1 handler transaction hash: {tx_hash}.");
        }

        match result {
            Ok((tx_execution_info, state_maps)) => {
                if let Some(ref revert_error) = tx_execution_info.revert_error {
                    warn!(
                        "Transaction {} is reverted while accepted. Revert Error: {:?}",
                        input_tx.tx_hash(),
                        revert_error,
                    );
                }
                let (tx_index, duplicate_tx_hash) =
                    execution_data.execution_infos.insert_full(tx_hash, tx_execution_info);
                assert_eq!(duplicate_tx_hash, None, "Duplicate transaction: {tx_hash}.");

                // Skip sending the pre confirmed executed transactions, receipts and state diffs
                // during validation flow. In validate flow pre_confirmed_tx_sender is None.
                if let Some(pre_confirmed_tx_sender) = pre_confirmed_tx_sender {
                    let tx_receipt = StarknetClientTransactionReceipt::from((
                        tx_hash,
                        tx_index,
                        // TODO(noamsp): Consider using tx_execution_info and moving the line that
                        // consumes it below this (if it doesn't change functionality).
                        &execution_data.execution_infos[&tx_hash],
                        optional_l1_handler_tx,
                    ));

                    let tx_state_diff = StarknetClientStateDiff::from(state_maps).0;

                    let result = pre_confirmed_tx_sender.try_send((
                        input_tx.clone(),
                        tx_receipt,
                        tx_state_diff,
                    ));
                    if result.is_err() {
                        // We continue with block building even if sending data to The
                        // PreconfirmedBlockWriter fails because it is not critical
                        // for the block building process.
                        warn!("Sending data to preconfirmed block writer failed.");
                    }
                }
            }
            Err(err) => {
                info!(
                    "Transaction {} failed to execute with error: {}.",
                    tx_hash,
                    err.log_compatible_to_string()
                );
                let is_new_entry = execution_data.rejected_tx_hashes.insert(tx_hash);
                assert!(is_new_entry, "Duplicate rejected transaction hash: {tx_hash}.");
            }
        }
    }

    Ok(())
}

pub struct BlockMetadata {
    pub block_info: BlockInfo,
    pub retrospective_block_hash: Option<BlockHashAndNumber>,
}

// Type definitions for the abort channel required to abort the block builder.
pub type AbortSignalSender = tokio::sync::oneshot::Sender<()>;
pub type BatcherWorkerPool =
    Arc<WorkerPool<CachedState<StateReaderAndContractManager<PapyrusReader>>>>;

/// The BlockBuilderFactoryTrait is responsible for creating a new block builder.
#[cfg_attr(test, automock)]
pub trait BlockBuilderFactoryTrait: Send + Sync {
    // TODO(noamsp): Investigate and remove this clippy warning.
    #[allow(clippy::result_large_err, clippy::too_many_arguments)]
    fn create_block_builder(
        &self,
        block_metadata: BlockMetadata,
        execution_params: BlockBuilderExecutionParams,
        tx_provider: Box<dyn TransactionProvider>,
        output_content_sender: Option<
            tokio::sync::mpsc::UnboundedSender<InternalConsensusTransaction>,
        >,
        candidate_tx_sender: Option<CandidateTxSender>,
        pre_confirmed_tx_sender: Option<PreconfirmedTxSender>,
        runtime: tokio::runtime::Handle,
    ) -> BlockBuilderResult<(Box<dyn BlockBuilderTrait>, AbortSignalSender)>;
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BlockBuilderConfig {
    pub chain_info: ChainInfo,
    pub execute_config: WorkerPoolConfig,
    pub bouncer_config: BouncerConfig,
    pub n_concurrent_txs: usize,
    pub tx_polling_interval_millis: u64,
    pub versioned_constants_overrides: VersionedConstantsOverrides,
}

impl Default for BlockBuilderConfig {
    fn default() -> Self {
        Self {
            // TODO(AlonH): update the default values once the actual values are known.
            chain_info: ChainInfo::default(),
            execute_config: WorkerPoolConfig::default(),
            bouncer_config: BouncerConfig::default(),
            n_concurrent_txs: 100,
            tx_polling_interval_millis: 1,
            versioned_constants_overrides: VersionedConstantsOverrides::default(),
        }
    }
}

impl SerializeConfig for BlockBuilderConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = prepend_sub_config_name(self.chain_info.dump(), "chain_info");
        dump.append(&mut prepend_sub_config_name(self.execute_config.dump(), "execute_config"));
        dump.append(&mut prepend_sub_config_name(self.bouncer_config.dump(), "bouncer_config"));
        dump.append(&mut BTreeMap::from([ser_param(
            "n_concurrent_txs",
            &self.n_concurrent_txs,
            "Number of transactions in each request from the tx_provider.",
            ParamPrivacyInput::Public,
        )]));
        dump.append(&mut BTreeMap::from([ser_param(
            "tx_polling_interval_millis",
            &self.tx_polling_interval_millis,
            "Time to wait (in milliseconds) between transaction requests when the previous \
             request returned no transactions.",
            ParamPrivacyInput::Public,
        )]));
        dump.append(&mut prepend_sub_config_name(
            self.versioned_constants_overrides.dump(),
            "versioned_constants_overrides",
        ));
        dump
    }
}

pub struct BlockBuilderFactory {
    pub block_builder_config: BlockBuilderConfig,
    pub storage_reader: StorageReader,
    pub contract_class_manager: ContractClassManager,
    pub class_manager_client: SharedClassManagerClient,
    pub worker_pool: BatcherWorkerPool,
}

impl BlockBuilderFactory {
    // TODO(noamsp): Investigate and remove this clippy warning.
    #[allow(clippy::result_large_err)]
    fn preprocess_and_create_transaction_executor(
        &self,
        block_metadata: BlockMetadata,
        runtime: tokio::runtime::Handle,
    ) -> BlockBuilderResult<
        ConcurrentTransactionExecutor<StateReaderAndContractManager<PapyrusReader>>,
    > {
        let height = block_metadata.block_info.block_number;
        let block_builder_config = self.block_builder_config.clone();
        let versioned_constants = VersionedConstants::get_versioned_constants(
            block_builder_config.versioned_constants_overrides,
        );
        let block_context = BlockContext::new(
            block_metadata.block_info,
            block_builder_config.chain_info,
            versioned_constants,
            block_builder_config.bouncer_config,
        );

        let class_reader = Some(ClassReader { reader: self.class_manager_client.clone(), runtime });
        let papyrus_reader =
            PapyrusReader::new_with_class_reader(self.storage_reader.clone(), height, class_reader);
        let state_reader = StateReaderAndContractManager {
            state_reader: papyrus_reader,
            contract_class_manager: self.contract_class_manager.clone(),
        };

        let executor = ConcurrentTransactionExecutor::start_block(
            state_reader,
            block_context,
            block_metadata.retrospective_block_hash,
            self.worker_pool.clone(),
            None,
        )?;

        Ok(executor)
    }
}

impl BlockBuilderFactoryTrait for BlockBuilderFactory {
    fn create_block_builder(
        &self,
        block_metadata: BlockMetadata,
        execution_params: BlockBuilderExecutionParams,
        tx_provider: Box<dyn TransactionProvider>,
        output_content_sender: Option<
            tokio::sync::mpsc::UnboundedSender<InternalConsensusTransaction>,
        >,
        candidate_tx_sender: Option<CandidateTxSender>,
        pre_confirmed_tx_sender: Option<PreconfirmedTxSender>,
        runtime: tokio::runtime::Handle,
    ) -> BlockBuilderResult<(Box<dyn BlockBuilderTrait>, AbortSignalSender)> {
        let executor = self.preprocess_and_create_transaction_executor(block_metadata, runtime)?;
        let (abort_signal_sender, abort_signal_receiver) = tokio::sync::oneshot::channel();
        let transaction_converter = TransactionConverter::new(
            self.class_manager_client.clone(),
            self.block_builder_config.chain_info.chain_id.clone(),
        );
        let block_builder = Box::new(BlockBuilder::new(
            executor,
            tx_provider,
            output_content_sender,
            candidate_tx_sender,
            pre_confirmed_tx_sender,
            abort_signal_receiver,
            transaction_converter,
            self.block_builder_config.n_concurrent_txs,
            self.block_builder_config.tx_polling_interval_millis,
            execution_params,
        ));
        Ok((block_builder, abort_signal_sender))
    }
}

/// Supplementary information for use by downstream services.
#[cfg_attr(test, derive(Clone))]
#[derive(Debug, Default, PartialEq)]
pub struct BlockTransactionExecutionData {
    pub execution_infos: IndexMap<TransactionHash, TransactionExecutionInfo>,
    pub rejected_tx_hashes: IndexSet<TransactionHash>,
    pub consumed_l1_handler_tx_hashes: IndexSet<TransactionHash>,
}

impl BlockTransactionExecutionData {
    /// Removes the last txs with the given hashes from the execution data.
    fn remove_last_txs(&mut self, tx_hashes: &[TransactionHash]) {
        for tx_hash in tx_hashes.iter().rev() {
            remove_last_map(&mut self.execution_infos, tx_hash);
            remove_last_set(&mut self.rejected_tx_hashes, tx_hash);
            remove_last_set(&mut self.consumed_l1_handler_tx_hashes, tx_hash);
        }
    }

    fn l2_gas_used(&self) -> GasAmount {
        let mut res = GasAmount::ZERO;
        for execution_info in self.execution_infos.values() {
            res =
                res.checked_add(execution_info.receipt.gas.l2_gas).expect("Total L2 gas overflow.");
        }

        res
    }
}

/// Removes the tx_hash from the map, if it exists.
/// Verifies that the removed transaction is the last one in the map.
fn remove_last_map<V>(map: &mut IndexMap<TransactionHash, V>, tx_hash: &TransactionHash) {
    if let Some((idx, _, _)) = map.swap_remove_full(tx_hash) {
        assert_eq!(idx, map.len(), "The removed txs must be the last ones.");
    }
}

/// Removes the tx_hash from the set, if it exists.
/// Verifies that the removed transaction is the last one in the set.
fn remove_last_set(set: &mut IndexSet<TransactionHash>, tx_hash: &TransactionHash) {
    if let Some((idx, _)) = set.swap_remove_full(tx_hash) {
        assert_eq!(idx, set.len(), "The removed txs must be the last ones.");
    }
}
