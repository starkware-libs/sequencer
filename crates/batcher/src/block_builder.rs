use std::collections::BTreeMap;

use async_trait::async_trait;
use blockifier::blockifier::block::{BlockInfo, GasPrices};
use blockifier::blockifier::config::TransactionExecutorConfig;
use blockifier::blockifier::transaction_executor::{
    TransactionExecutor,
    TransactionExecutorError as BlockifierTransactionExecutorError,
    TransactionExecutorResult,
    VisitedSegmentsMapping,
};
use blockifier::bouncer::{BouncerConfig, BouncerWeights};
use blockifier::context::{BlockContext, ChainInfo};
use blockifier::state::cached_state::CommitmentStateDiff;
use blockifier::state::errors::StateError;
use blockifier::state::global_cache::GlobalContractCache;
use blockifier::transaction::errors::TransactionExecutionError as BlockifierTransactionExecutionError;
use blockifier::transaction::objects::TransactionExecutionInfo;
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use blockifier::versioned_constants::{VersionedConstants, VersionedConstantsOverrides};
use indexmap::IndexMap;
#[cfg(test)]
use mockall::automock;
use papyrus_config::dumping::{
    append_sub_config_name,
    ser_optional_sub_config,
    ser_param,
    SerializeConfig,
};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use papyrus_state_reader::papyrus_state::PapyrusReader;
use papyrus_storage::StorageReader;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHashAndNumber, BlockNumber, BlockTimestamp, NonzeroGasPrice};
use starknet_api::core::ContractAddress;
use starknet_api::executable_transaction::Transaction;
use starknet_api::transaction::TransactionHash;
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, error, info, trace};

use crate::transaction_executor::TransactionExecutorTrait;
use crate::transaction_provider::{NextTxs, TransactionProvider, TransactionProviderError};

#[derive(Debug, Error)]
pub enum BlockBuilderError {
    #[error(transparent)]
    BadTimestamp(#[from] std::num::TryFromIntError),
    #[error(transparent)]
    BlockifierStateError(#[from] StateError),
    #[error(transparent)]
    ExecutorError(#[from] BlockifierTransactionExecutorError),
    #[error(transparent)]
    GetTransactionError(#[from] TransactionProviderError),
    #[error(transparent)]
    TransactionExecutionError(#[from] BlockifierTransactionExecutionError),
    #[error(transparent)]
    StreamTransactionsError(#[from] tokio::sync::mpsc::error::SendError<Transaction>),
    #[error("Build block with fail_on_err mode, failed on error {}.", _0)]
    FailOnError(BlockifierTransactionExecutorError),
}

pub type BlockBuilderResult<T> = Result<T, BlockBuilderError>;

#[cfg_attr(test, derive(Clone))]
#[derive(Debug, PartialEq)]
pub struct BlockExecutionArtifacts {
    pub execution_infos: IndexMap<TransactionHash, TransactionExecutionInfo>,
    pub commitment_state_diff: CommitmentStateDiff,
    pub visited_segments_mapping: VisitedSegmentsMapping,
    pub bouncer_weights: BouncerWeights,
}

/// The BlockBuilderTrait is responsible for building a new block from transactions provided by the
/// tx_provider. The block building will stop at time deadline.
/// The transactions that were added to the block will be streamed to the output_content_sender.
#[cfg_attr(test, automock)]
#[async_trait]
pub trait BlockBuilderTrait: Send {
    async fn build_block(&mut self) -> BlockBuilderResult<BlockExecutionArtifacts>;
}

pub struct BlockBuilder {
    // TODO(Yael 14/10/2024): make the executor thread safe and delete this mutex.
    executor: Mutex<Box<dyn TransactionExecutorTrait>>,
    tx_chunk_size: usize,
    deadline: tokio::time::Instant,
    tx_provider: Box<dyn TransactionProvider>,
    output_content_sender: Option<tokio::sync::mpsc::UnboundedSender<Transaction>>,
    fail_on_err: bool,
}

impl BlockBuilder {
    pub fn new(
        executor: Box<dyn TransactionExecutorTrait>,
        tx_chunk_size: usize,
        deadline: tokio::time::Instant,
        tx_provider: Box<dyn TransactionProvider>,
        output_content_sender: Option<tokio::sync::mpsc::UnboundedSender<Transaction>>,
        fail_on_err: bool,
    ) -> Self {
        Self {
            executor: Mutex::new(executor),
            tx_chunk_size,
            deadline,
            tx_provider,
            output_content_sender,
            fail_on_err,
        }
    }
}

#[async_trait]
impl BlockBuilderTrait for BlockBuilder {
    async fn build_block(&mut self) -> BlockBuilderResult<BlockExecutionArtifacts> {
        let mut block_is_full = false;
        let mut execution_infos = IndexMap::new();
        // TODO(yael 6/10/2024): delete the timeout condition once the executor has a timeout
        while !block_is_full && tokio::time::Instant::now() < self.deadline {
            let next_txs = self.tx_provider.get_txs(self.tx_chunk_size).await?;
            let next_tx_chunk = match next_txs {
                NextTxs::Txs(txs) => txs,
                NextTxs::End => break,
            };
            debug!("Got {} transactions from the transaction provider.", next_tx_chunk.len());
            if next_tx_chunk.is_empty() {
                // TODO: Consider what is the best sleep duration.
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                continue;
            }

            let mut executor_input_chunk = vec![];
            for tx in &next_tx_chunk {
                // TODO(yair): Avoid this clone.
                executor_input_chunk.push(BlockifierTransaction::from(tx.clone()));
            }
            let results = self.executor.lock().await.add_txs_to_block(&executor_input_chunk);
            trace!("Transaction execution results: {:?}", results);
            block_is_full = collect_execution_results_and_stream_txs(
                next_tx_chunk,
                results,
                &mut execution_infos,
                &self.output_content_sender,
                self.fail_on_err,
            )
            .await?;
        }
        let (commitment_state_diff, visited_segments_mapping, bouncer_weights) =
            self.executor.lock().await.close_block()?;
        Ok(BlockExecutionArtifacts {
            execution_infos,
            commitment_state_diff,
            visited_segments_mapping,
            bouncer_weights,
        })
    }
}

/// Returns true if the block is full and should be closed, false otherwise.
async fn collect_execution_results_and_stream_txs(
    tx_chunk: Vec<Transaction>,
    results: Vec<TransactionExecutorResult<TransactionExecutionInfo>>,
    execution_infos: &mut IndexMap<TransactionHash, TransactionExecutionInfo>,
    output_content_sender: &Option<tokio::sync::mpsc::UnboundedSender<Transaction>>,
    fail_on_err: bool,
) -> BlockBuilderResult<bool> {
    for (input_tx, result) in tx_chunk.into_iter().zip(results.into_iter()) {
        match result {
            Ok(tx_execution_info) => {
                execution_infos.insert(input_tx.tx_hash(), tx_execution_info);
                if let Some(output_content_sender) = output_content_sender {
                    output_content_sender.send(input_tx)?;
                }
            }
            // TODO(yael 18/9/2024): add timeout error handling here once this
            // feature is added.
            Err(BlockifierTransactionExecutorError::BlockFull) if !fail_on_err => {
                info!("Block is full");
                return Ok(true);
            }
            Err(err) => {
                debug!("Transaction {:?} failed with error: {}.", input_tx, err);
                if fail_on_err {
                    return Err(BlockBuilderError::FailOnError(err));
                }
            }
        }
    }
    Ok(false)
}

/// The BlockBuilderFactoryTrait is responsible for creating a new block builder.
#[cfg_attr(test, automock)]
pub trait BlockBuilderFactoryTrait {
    fn create_block_builder(
        &self,
        height: BlockNumber,
        retrospective_block_hash: Option<BlockHashAndNumber>,
        deadline: tokio::time::Instant,
        tx_provider: Box<dyn TransactionProvider>,
        output_content_sender: Option<tokio::sync::mpsc::UnboundedSender<Transaction>>,
        fail_on_err: bool,
    ) -> BlockBuilderResult<Box<dyn BlockBuilderTrait>>;
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BlockBuilderConfig {
    // TODO(Yael 1/10/2024): add to config pointers
    pub chain_info: ChainInfo,
    pub execute_config: TransactionExecutorConfig,
    pub bouncer_config: BouncerConfig,
    pub sequencer_address: ContractAddress,
    pub use_kzg_da: bool,
    pub tx_chunk_size: usize,
    pub versioned_constants_overrides: Option<VersionedConstantsOverrides>,
}

impl Default for BlockBuilderConfig {
    fn default() -> Self {
        Self {
            // TODO: update the default values once the actual values are known.
            chain_info: ChainInfo::default(),
            execute_config: TransactionExecutorConfig::default(),
            bouncer_config: BouncerConfig::default(),
            sequencer_address: ContractAddress::default(),
            use_kzg_da: true,
            tx_chunk_size: 100,
            versioned_constants_overrides: None,
        }
    }
}

impl SerializeConfig for BlockBuilderConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = append_sub_config_name(self.chain_info.dump(), "chain_info");
        dump.append(&mut append_sub_config_name(self.execute_config.dump(), "execute_config"));
        dump.append(&mut append_sub_config_name(self.bouncer_config.dump(), "bouncer_config"));
        dump.append(&mut BTreeMap::from([ser_param(
            "sequencer_address",
            &self.sequencer_address,
            "The address of the sequencer.",
            ParamPrivacyInput::Public,
        )]));
        dump.append(&mut BTreeMap::from([ser_param(
            "use_kzg_da",
            &self.use_kzg_da,
            "Indicates whether the kzg mechanism is used for data availability.",
            ParamPrivacyInput::Public,
        )]));
        dump.append(&mut BTreeMap::from([ser_param(
            "tx_chunk_size",
            &self.tx_chunk_size,
            "The size of the transaction chunk.",
            ParamPrivacyInput::Public,
        )]));
        dump.append(&mut ser_optional_sub_config(
            &self.versioned_constants_overrides,
            "versioned_constants_overrides",
        ));
        dump
    }
}

pub struct BlockBuilderFactory {
    pub block_builder_config: BlockBuilderConfig,
    pub storage_reader: StorageReader,
    pub global_class_hash_to_class: GlobalContractCache,
}

impl BlockBuilderFactory {
    fn preprocess_and_create_transaction_executor(
        &self,
        height: BlockNumber,
        retrospective_block_hash: Option<BlockHashAndNumber>,
    ) -> BlockBuilderResult<TransactionExecutor<PapyrusReader>> {
        let block_builder_config = self.block_builder_config.clone();
        let next_block_info = BlockInfo {
            block_number: height,
            block_timestamp: BlockTimestamp(chrono::Utc::now().timestamp().try_into()?),
            sequencer_address: block_builder_config.sequencer_address,
            // TODO (yael 7/10/2024): add logic to compute gas prices
            gas_prices: {
                let tmp_val = NonzeroGasPrice::MIN;
                GasPrices::new(tmp_val, tmp_val, tmp_val, tmp_val, tmp_val, tmp_val)
            },
            use_kzg_da: block_builder_config.use_kzg_da,
        };
        let versioned_constants = VersionedConstants::latest_with_overrides(
            block_builder_config.versioned_constants_overrides,
        );
        let block_context = BlockContext::new(
            next_block_info,
            block_builder_config.chain_info,
            versioned_constants,
            block_builder_config.bouncer_config,
        );

        // TODO(Yael: 8/9/2024) Need to reconsider which StateReader to use. the papyrus execution
        // state reader does not implement the Sync trait since it is using cell so I used
        // the blockifier state reader instead. Also the blockifier reader is implementing a global
        // cache.
        let state_reader = PapyrusReader::new(
            self.storage_reader.clone(),
            height,
            // TODO(Yael 18/9/2024): dont forget to flush the cached_state cache into the global
            // cache on decision_reached.
            self.global_class_hash_to_class.clone(),
        );

        let executor = TransactionExecutor::pre_process_and_create(
            state_reader,
            block_context,
            retrospective_block_hash,
            block_builder_config.execute_config,
        )?;

        Ok(executor)
    }
}

impl BlockBuilderFactoryTrait for BlockBuilderFactory {
    fn create_block_builder(
        &self,
        height: BlockNumber,
        retrospective_block_hash: Option<BlockHashAndNumber>,
        deadline: tokio::time::Instant,
        tx_provider: Box<dyn TransactionProvider>,
        output_content_sender: Option<tokio::sync::mpsc::UnboundedSender<Transaction>>,
        fail_on_err: bool,
    ) -> BlockBuilderResult<Box<dyn BlockBuilderTrait>> {
        let executor =
            self.preprocess_and_create_transaction_executor(height, retrospective_block_hash)?;
        Ok(Box::new(BlockBuilder::new(
            Box::new(executor),
            self.block_builder_config.tx_chunk_size,
            deadline,
            tx_provider,
            output_content_sender,
            fail_on_err,
        )))
    }
}
