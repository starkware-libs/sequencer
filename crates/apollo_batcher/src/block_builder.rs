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
use blockifier::blockifier::config::TransactionExecutorConfig;
use blockifier::blockifier::transaction_executor::{
    BlockExecutionSummary,
    TransactionExecutor,
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
use tokio::sync::Mutex;
use tracing::{debug, error, info, trace};

use crate::block_builder::FailOnErrorCause::L1HandlerTransactionValidationFailed;
use crate::metrics::FULL_BLOCKS;
use crate::pre_confirmed_block_writer::{ExecutedTxSender, PreConfirmedTxSender};
use crate::transaction_executor::TransactionExecutorTrait;
use crate::transaction_provider::{NextTxs, TransactionProvider, TransactionProviderError};

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
    pub casm_hash_computation_data: CasmHashComputationData,
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
    pub fail_on_err: bool,
}

pub struct BlockBuilder {
    // TODO(Yael 14/10/2024): make the executor thread safe and delete this mutex.
    executor: Arc<Mutex<dyn TransactionExecutorTrait>>,
    tx_provider: Box<dyn TransactionProvider>,
    output_content_sender: Option<tokio::sync::mpsc::UnboundedSender<InternalConsensusTransaction>>,
    // Optional senders because they are not used during validation flow.
    pre_confirmed_tx_sender: Option<PreConfirmedTxSender>,
    _executed_tx_sender: Option<ExecutedTxSender>,
    abort_signal_receiver: tokio::sync::oneshot::Receiver<()>,
    transaction_converter: TransactionConverter,

    // Parameters to configure the block builder behavior.
    tx_chunk_size: usize,
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
        pre_confirmed_tx_sender: Option<PreConfirmedTxSender>,
        executed_tx_sender: Option<ExecutedTxSender>,
        abort_signal_receiver: tokio::sync::oneshot::Receiver<()>,
        transaction_converter: TransactionConverter,
        tx_chunk_size: usize,
        tx_polling_interval_millis: u64,
        execution_params: BlockBuilderExecutionParams,
    ) -> Self {
        let executor = Arc::new(Mutex::new(executor));
        Self {
            executor,
            tx_provider,
            output_content_sender,
            pre_confirmed_tx_sender,
            _executed_tx_sender: executed_tx_sender,
            abort_signal_receiver,
            transaction_converter,
            tx_chunk_size,
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
        let mut block_is_full = false;
        let mut l2_gas_used = GasAmount::ZERO;
        let mut execution_data = BlockTransactionExecutionData::default();
        // TODO(yael 6/10/2024): delete the timeout condition once the executor has a timeout
        while !block_is_full {
            if tokio::time::Instant::now() >= self.execution_params.deadline {
                info!("Block builder deadline reached.");
                if self.execution_params.fail_on_err {
                    return Err(BlockBuilderError::FailOnError(FailOnErrorCause::DeadlineReached));
                }
                break;
            }
            if self.abort_signal_receiver.try_recv().is_ok() {
                info!("Received abort signal. Aborting block builder.");
                return Err(BlockBuilderError::Aborted);
            }
            let next_txs = match self.tx_provider.get_txs(self.tx_chunk_size).await {
                Err(e @ TransactionProviderError::L1HandlerTransactionValidationFailed { .. })
                    if self.execution_params.fail_on_err =>
                {
                    return Err(BlockBuilderError::FailOnError(
                        L1HandlerTransactionValidationFailed(e),
                    ));
                }
                Err(err) => {
                    error!("Failed to get transactions from the transaction provider: {:?}", err);
                    return Err(err.into());
                }
                Ok(result) => result,
            };
            let next_tx_chunk = match next_txs {
                NextTxs::Txs(txs) => txs,
                NextTxs::End => break,
            };
            debug!("Got {} transactions from the transaction provider.", next_tx_chunk.len());
            if next_tx_chunk.is_empty() {
                tokio::time::sleep(tokio::time::Duration::from_millis(
                    self.tx_polling_interval_millis,
                ))
                .await;
                continue;
            }

            if let Some(pre_confirmed_tx_sender) = &self.pre_confirmed_tx_sender {
                let tx_hashes: Vec<TransactionHash> =
                    next_tx_chunk.iter().map(|tx| tx.tx_hash()).collect();
                if let Err(e) = pre_confirmed_tx_sender.send(tx_hashes) {
                    error!(
                        "Failed to send the next chunk tx hashes to the pre confirmed tx sender: \
                         {}",
                        e
                    );
                }
            }

            let tx_convert_futures = next_tx_chunk.iter().map(|tx| async {
                convert_to_executable_blockifier_tx(&self.transaction_converter, tx.clone()).await
            });
            let executor_input_chunk = futures::future::try_join_all(tx_convert_futures).await?;

            // Execute the transactions on a separate thread pool to avoid blocking the executor
            // while waiting on `block_on` calls.
            debug!(
                "Starting execution of a chunk with {} transactions.",
                executor_input_chunk.len()
            );
            let executor = self.executor.clone();
            let block_deadline = self.execution_params.deadline;
            let results = tokio::task::spawn_blocking(move || {
                executor
                    .try_lock() // Acquire the lock in a sync manner.
                    .expect("Only a single task should use the executor.")
                    .add_txs_to_block(executor_input_chunk.as_slice(), block_deadline)
            })
            .await
            .expect("Failed to spawn blocking executor task.");
            debug!("Finished execution of transactions chunk.");
            trace!("Transaction execution results: {:?}", results);
            block_is_full = collect_execution_results_and_stream_txs(
                next_tx_chunk,
                results,
                &mut l2_gas_used,
                &mut execution_data,
                &self.output_content_sender,
                self.execution_params.fail_on_err,
            )
            .await?;
        }
        let BlockExecutionSummary {
            state_diff,
            compressed_state_diff,
            bouncer_weights,
            casm_hash_computation_data,
        } = self.executor.lock().await.close_block()?;
        Ok(BlockExecutionArtifacts {
            execution_data,
            commitment_state_diff: state_diff,
            compressed_state_diff,
            bouncer_weights,
            l2_gas_used,
            casm_hash_computation_data,
        })
    }
}

async fn convert_to_executable_blockifier_tx(
    transaction_converter: &TransactionConverter,
    tx: InternalConsensusTransaction,
) -> TransactionConverterResult<BlockifierTransaction> {
    let executable_tx =
        transaction_converter.convert_internal_consensus_tx_to_executable_tx(tx).await?;
    Ok(BlockifierTransaction::new_for_sequencing(executable_tx))
}

/// Returns true if the block is full and should be closed, false otherwise.
async fn collect_execution_results_and_stream_txs(
    tx_chunk: Vec<InternalConsensusTransaction>,
    results: Vec<TransactionExecutorResult<TransactionExecutionInfo>>,
    l2_gas_used: &mut GasAmount,
    execution_data: &mut BlockTransactionExecutionData,
    output_content_sender: &Option<
        tokio::sync::mpsc::UnboundedSender<InternalConsensusTransaction>,
    >,
    fail_on_err: bool,
) -> BlockBuilderResult<bool> {
    assert!(
        results.len() <= tx_chunk.len(),
        "The number of results should be less than or equal to the number of transactions."
    );
    let mut block_is_full = false;
    // If the block is full, we won't get an error from the executor. We will just get only the
    // results of the transactions that were executed before the block was full.
    // see [TransactionExecutor::execute_txs].
    if results.len() < tx_chunk.len() {
        info!("Block is full.");
        if fail_on_err {
            return Err(BlockBuilderError::FailOnError(FailOnErrorCause::BlockFull));
        } else {
            FULL_BLOCKS.increment(1);
            block_is_full = true;
        }
    }
    for (input_tx, result) in tx_chunk.into_iter().zip(results.into_iter()) {
        let tx_hash = input_tx.tx_hash();

        // Insert the tx_hash into the appropriate collection if it's an L1_Handler transaction.
        if let InternalConsensusTransaction::L1Handler(_) = input_tx {
            execution_data.consumed_l1_handler_tx_hashes.insert(tx_hash);
        }

        match result {
            Ok(tx_execution_info) => {
                *l2_gas_used = l2_gas_used
                    .checked_add(tx_execution_info.receipt.gas.l2_gas)
                    .expect("Total L2 gas overflow.");

                execution_data.execution_infos.insert(tx_hash, tx_execution_info);

                if let Some(output_content_sender) = output_content_sender {
                    output_content_sender.send(input_tx)?;
                }
            }
            // TODO(yael 18/9/2024): add timeout error handling here once this
            // feature is added.
            Err(err) => {
                debug!(
                    "Transaction {} failed with error: {}.",
                    tx_hash,
                    err.log_compatible_to_string()
                );
                if fail_on_err {
                    return Err(BlockBuilderError::FailOnError(
                        FailOnErrorCause::TransactionFailed(err),
                    ));
                }
                execution_data.rejected_tx_hashes.insert(tx_hash);
            }
        }
    }
    Ok(block_is_full)
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
        pre_confirmed_tx_sender: Option<PreConfirmedTxSender>,
        executed_tx_sender: Option<ExecutedTxSender>,
        runtime: tokio::runtime::Handle,
    ) -> BlockBuilderResult<(Box<dyn BlockBuilderTrait>, AbortSignalSender)>;
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BlockBuilderConfig {
    pub chain_info: ChainInfo,
    pub execute_config: TransactionExecutorConfig,
    pub bouncer_config: BouncerConfig,
    pub tx_chunk_size: usize,
    pub tx_polling_interval_millis: u64,
    pub versioned_constants_overrides: VersionedConstantsOverrides,
}

impl Default for BlockBuilderConfig {
    fn default() -> Self {
        Self {
            // TODO(AlonH): update the default values once the actual values are known.
            chain_info: ChainInfo::default(),
            execute_config: TransactionExecutorConfig::default(),
            bouncer_config: BouncerConfig::default(),
            tx_chunk_size: 100,
            tx_polling_interval_millis: 100,
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
            "tx_chunk_size",
            &self.tx_chunk_size,
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
    ) -> BlockBuilderResult<TransactionExecutor<StateReaderAndContractManager<PapyrusReader>>> {
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

        let executor = TransactionExecutor::pre_process_and_create_with_pool(
            state_reader,
            block_context,
            block_metadata.retrospective_block_hash,
            block_builder_config.execute_config,
            Some(self.worker_pool.clone()),
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
        pre_confirmed_tx_sender: Option<PreConfirmedTxSender>,
        executed_tx_sender: Option<ExecutedTxSender>,
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
            pre_confirmed_tx_sender,
            executed_tx_sender,
            abort_signal_receiver,
            transaction_converter,
            self.block_builder_config.tx_chunk_size,
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
    pub rejected_tx_hashes: HashSet<TransactionHash>,
    pub consumed_l1_handler_tx_hashes: IndexSet<TransactionHash>,
}
