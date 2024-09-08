use std::collections::HashMap;
use std::num::NonZeroU128;

use async_trait::async_trait;
use blockifier::blockifier::block::{BlockInfo, BlockNumberHashPair, GasPrices};
use blockifier::blockifier::config::TransactionExecutorConfig;
use blockifier::blockifier::transaction_executor::{
    TransactionExecutor,
    TransactionExecutorResult,
    VisitedSegmentsMapping,
};
use blockifier::bouncer::{BouncerConfig, BouncerWeights};
use blockifier::context::{BlockContext, ChainInfo};
use blockifier::state::cached_state::CommitmentStateDiff;
use blockifier::state::errors::StateError;
use blockifier::state::global_cache::GlobalContractCache;
use blockifier::state::state_api::StateReader;
use blockifier::transaction::objects::TransactionExecutionInfo;
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use blockifier::versioned_constants::{VersionedConstants, VersionedConstantsOverrides};
#[cfg(test)]
use mockall::automock;
use papyrus_storage::StorageReader;
use starknet_api::block::{BlockNumber, BlockTimestamp};
use starknet_api::core::ContractAddress;
use starknet_api::executable_transaction::Transaction;
use starknet_api::transaction::TransactionHash;
use thiserror::Error;
use tokio::sync::Mutex;

use crate::papyrus_state::PapyrusReader;
use crate::proposal_manager::InputTxStream;

pub struct BlockBuilder {
    _executor: Mutex<Box<dyn BlockifierTransactionExecutorTrait>>,
    _txs_chunk_size: usize,
}

#[derive(Debug, Error)]
pub enum BlockBuilderError {
    #[error(transparent)]
    BadTimestamp(#[from] std::num::TryFromIntError),
    #[error(transparent)]
    BlockifierStateError(#[from] StateError),
}

pub type BlockBuilderResult<T> = Result<T, BlockBuilderError>;

#[derive(Clone)]
pub struct ExecutionConfig {
    // TODO(Yael 1/10/2024): add to config pointers
    pub chain_info: ChainInfo,
    pub execute_config: TransactionExecutorConfig,
    pub bouncer_config: BouncerConfig,
    pub sequencer_address: ContractAddress,
    pub use_kzg_da: bool,
    pub txs_chunk_size: usize,
    pub versioned_constants_overrides: VersionedConstantsOverrides,
}

#[async_trait]
impl BlockBuilderTrait for BlockBuilder {
    async fn build_block(
        &mut self,
        _deadline: tokio::time::Instant,
        _mempool_tx_stream: InputTxStream,
        _output_content_sender: tokio::sync::mpsc::Sender<Transaction>,
    ) -> BlockBuilderResult<BlockExecutionArtifacts> {
        todo!();
    }
}

// TODO (yael 22/9/2024): implement this function for the next milestone
pub fn get_gas_prices() -> GasPrices {
    let one = NonZeroU128::new(1).unwrap();
    // TODO: L1 gas prices should be updated priodically and not necessarily on each block
    GasPrices::new(one, one, one, one, one, one)
}

#[derive(Default, Debug, PartialEq)]
pub struct BlockExecutionArtifacts {
    // TODO(yael): what is the best id for mapping? tx_hash or tx_id? depends on the orchestrator
    // needs.
    pub execution_infos: HashMap<TransactionHash, TransactionExecutionInfo>,
    pub commitment_state_diff: CommitmentStateDiff,
    pub visited_segments_mapping: VisitedSegmentsMapping,
    pub bouncer_weights: BouncerWeights,
}

#[async_trait]
impl BlockBuilderTrait for Box<dyn BlockBuilderTrait> {
    async fn build_block(
        &mut self,
        deadline: tokio::time::Instant,
        tx_stream: InputTxStream,
        output_content_sender: tokio::sync::mpsc::Sender<Transaction>,
    ) -> BlockBuilderResult<BlockExecutionArtifacts> {
        self.as_mut().build_block(deadline, tx_stream, output_content_sender).await
    }
}

#[cfg_attr(test, automock)]
#[async_trait]
pub trait BlockBuilderTrait: Send {
    async fn build_block(
        &mut self,
        deadline: tokio::time::Instant,
        tx_stream: InputTxStream,
        output_content_sender: tokio::sync::mpsc::Sender<Transaction>,
    ) -> BlockBuilderResult<BlockExecutionArtifacts>;
}

#[cfg_attr(test, automock)]
pub trait BlockBuilderFactoryTrait {
    fn create_block_builder(
        &self,
        next_block_number: BlockNumber,
        retrospective_block_hash: Option<BlockNumberHashPair>,
    ) -> BlockBuilderResult<Box<dyn BlockBuilderTrait>>;
}

pub struct BlockBuilderFactory {
    pub execution_config: ExecutionConfig,
    pub storage_reader: StorageReader,
    pub global_class_hash_to_class: GlobalContractCache,
}

impl BlockBuilderFactory {
    pub fn create_transaction_executor(
        &self,
        next_block_number: BlockNumber,
        retrospective_block_hash: Option<BlockNumberHashPair>,
    ) -> BlockBuilderResult<Box<dyn BlockifierTransactionExecutorTrait>> {
        let execution_config = self.execution_config.clone();
        let next_block_info = BlockInfo {
            block_number: next_block_number,
            block_timestamp: BlockTimestamp(chrono::Utc::now().timestamp().try_into()?),
            sequencer_address: execution_config.sequencer_address,
            gas_prices: get_gas_prices(),
            use_kzg_da: execution_config.use_kzg_da,
        };
        let block_context = BlockContext::new(
            next_block_info,
            execution_config.chain_info,
            VersionedConstants::get_versioned_constants(
                execution_config.versioned_constants_overrides,
            ),
            execution_config.bouncer_config,
        );

        // TODO(Yael: 8/9/2024) Need to reconsider which StateReader to use. the papyrus execution
        // state reader does not implement the Sync trait since it is using cell so I used
        // the blockifier state reader instead. Also the blockifier reader is implementing a global
        // cache.
        let state_reader = PapyrusReader::new(
            self.storage_reader.clone(),
            next_block_number,
            // TODO(Yael 18/9/2024): dont forget to flush the cached_state cache into the global
            // cache on decision_reached.
            self.global_class_hash_to_class.clone(),
        );

        let executor = TransactionExecutor::pre_process_and_create(
            state_reader,
            block_context,
            retrospective_block_hash,
            execution_config.execute_config,
        )?;

        Ok(Box::new(executor))
    }
}

impl BlockBuilderFactoryTrait for BlockBuilderFactory {
    fn create_block_builder(
        &self,
        next_block_number: BlockNumber,
        retrospective_block_hash: Option<BlockNumberHashPair>,
    ) -> BlockBuilderResult<Box<dyn BlockBuilderTrait>> {
        let executor =
            self.create_transaction_executor(next_block_number, retrospective_block_hash)?;
        Ok(Box::new(BlockBuilder {
            _executor: Mutex::new(executor),
            _txs_chunk_size: self.execution_config.txs_chunk_size,
        }))
    }
}

#[cfg_attr(test, automock)]
pub trait BlockifierTransactionExecutorTrait: Send {
    fn add_txs_to_block(
        &mut self,
        txs: &[BlockifierTransaction],
    ) -> Vec<TransactionExecutorResult<TransactionExecutionInfo>>;
    fn close_block(
        &mut self,
    ) -> TransactionExecutorResult<(CommitmentStateDiff, VisitedSegmentsMapping, BouncerWeights)>;
}

impl<S: StateReader + Send + Sync> BlockifierTransactionExecutorTrait for TransactionExecutor<S> {
    fn add_txs_to_block(
        &mut self,
        txs: &[BlockifierTransaction],
    ) -> Vec<TransactionExecutorResult<TransactionExecutionInfo>> {
        self.execute_txs(txs)
    }
    fn close_block(
        &mut self,
    ) -> TransactionExecutorResult<(CommitmentStateDiff, VisitedSegmentsMapping, BouncerWeights)>
    {
        self.finalize()
    }
}
