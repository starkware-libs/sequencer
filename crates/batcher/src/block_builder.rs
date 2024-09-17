use std::collections::HashMap;
use std::num::NonZeroU128;
use std::pin::Pin;

use blockifier::blockifier::block::{pre_process_block, BlockInfo, BlockNumberHashPair, GasPrices};
use blockifier::blockifier::config::TransactionExecutorConfig;
use blockifier::blockifier::transaction_executor::{
    TransactionExecutor,
    TransactionExecutorError as BlockifierTransactionExecutorError,
    TransactionExecutorTrait as BlockifierTransactionExecutorTrait,
    VisitedSegmentsMapping,
};
use blockifier::bouncer::{BouncerConfig, BouncerWeights};
use blockifier::context::{BlockContext, ChainInfo};
use blockifier::state::cached_state::{CachedState, CommitmentStateDiff};
use blockifier::state::errors::StateError;
use blockifier::state::global_cache::GlobalContractCache;
use blockifier::transaction::account_transaction::AccountTransaction;
use blockifier::transaction::errors::TransactionExecutionError as BlockifierTransactionExecutionError;
use blockifier::transaction::objects::TransactionExecutionInfo;
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use blockifier::versioned_constants::{VersionedConstants, VersionedConstantsOverrides};
use papyrus_storage::StorageReader;
use starknet_api::block::{BlockNumber, BlockTimestamp};
use starknet_api::core::ContractAddress;
use starknet_api::executable_transaction::Transaction;
use starknet_api::transaction::TransactionHash;
use thiserror::Error;
use tokio_stream::{Stream, StreamExt};
use tracing::{debug, info};

use crate::papyrus_state::PapyrusReader;

#[cfg(test)]
#[path = "block_builder_test.rs"]
pub mod block_builder_test;

pub struct BlockBuilder {
    executor: Box<dyn BlockifierTransactionExecutorTrait>,
    pub txs_chunk_size: usize,
}

#[derive(Debug, Error)]
pub enum BlockBuilderError {
    #[error(transparent)]
    BadTimestamp(#[from] std::num::TryFromIntError),
    #[error("No transactions sent.")]
    NoTransactions,
    #[error(transparent)]
    BlockifierStateError(#[from] StateError),
    #[error(transparent)]
    ExecutionError(#[from] BlockifierTransactionExecutorError),
    #[error(transparent)]
    TransactionExecutionError(#[from] BlockifierTransactionExecutionError),
    #[error(transparent)]
    StreamTransactionsError(#[from] tokio::sync::mpsc::error::SendError<Transaction>),
}

pub type BlockBuilderResult<T> = Result<T, BlockBuilderError>;

pub struct ExecutionConfig {
    pub chain_info: ChainInfo,
    pub execute_config: TransactionExecutorConfig,
    pub bouncer_config: BouncerConfig,
    pub sequencer_address: ContractAddress,
    pub use_kzg_da: bool,
    pub txs_chunk_size: usize,
    pub versioned_constants_overrides: VersionedConstantsOverrides,
}

pub struct ExecutionParams {
    pub execution_config: ExecutionConfig,
    pub global_class_hash_to_class: GlobalContractCache,
}

impl BlockBuilder {
    pub fn new(
        transaction_executor: Box<dyn BlockifierTransactionExecutorTrait>,
        txs_chunk_size: usize,
    ) -> BlockBuilderResult<Self> {
        Ok(BlockBuilder { executor: transaction_executor, txs_chunk_size })
    }

    pub fn create_transaction_executor(
        next_block_number: BlockNumber,
        storage_reader: StorageReader,
        retrospective_block_hash: Option<BlockNumberHashPair>,
        execution_params: ExecutionParams,
    ) -> BlockBuilderResult<Box<dyn BlockifierTransactionExecutorTrait>> {
        let execution_config = execution_params.execution_config;
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
            storage_reader,
            next_block_number,
            // TODO(Yael 18/9/2024): dont forget to flush the cached_state cache into the global
            // cache on decision_reached.
            execution_params.global_class_hash_to_class.clone(),
        );
        let mut state = CachedState::new(state_reader);

        pre_process_block(&mut state, retrospective_block_hash, next_block_number)?;

        Ok(Box::new(TransactionExecutor::new(
            state,
            block_context,
            execution_config.execute_config,
        )))
    }

    /// Adds transactions to a block and streams the transactions that were executed succeessfully.
    /// Returns the block artifacts if the block is done.
    pub async fn build_block<'a>(
        &mut self,
        deadline: tokio::time::Instant,
        mempool_tx_stream: impl Stream<Item = Transaction> + 'a,
        output_content_sender: tokio::sync::mpsc::Sender<Transaction>,
    ) -> BlockBuilderResult<BlockExecutionArtifacts> {
        let mut execution_infos = HashMap::new();
        tokio::pin!(mempool_tx_stream);
        let mut close_block = false;
        while !close_block && tokio::time::Instant::now() < deadline {
            let txs_chunk = self.get_txs_chunk(&mut mempool_tx_stream).await?;

            let results = self.executor.as_mut().add_txs_to_block(&txs_chunk.blockifier_format);
            for (mempool_tx, result) in
                txs_chunk.mempool_format.into_iter().zip(results.into_iter())
            {
                match result {
                    Ok(tx_execution_info) => {
                        execution_infos.insert(mempool_tx.tx_hash(), tx_execution_info);
                        output_content_sender.send(mempool_tx).await?;
                    }
                    Err(err) => match err {
                        // TODO(yael 18/9/2024): add timeout error handling here once this feature
                        // is added.
                        BlockifierTransactionExecutorError::BlockFull => {
                            info!("Block is full");
                            close_block = true;
                        }
                        _ => {
                            debug!("Transaction {:?} failed with error: {}.", mempool_tx, err);
                        }
                    },
                }
            }
        }
        let (commitment_state_diff, visited_segments_mapping, bouncer_weights) =
            self.executor.close_block()?;
        Ok(BlockExecutionArtifacts {
            execution_infos,
            commitment_state_diff,
            visited_segments_mapping,
            bouncer_weights,
        })

        // TODO how to update the mempool which transactions suceeded? maybe it's the
        // orchestrator's responsibilty
    }

    async fn get_txs_chunk<'a>(
        &mut self,
        mempool_tx_stream: &mut Pin<&'a mut (impl Stream<Item = Transaction> + 'a)>,
    ) -> BlockBuilderResult<TxsChunk> {
        let mut txs_in_blockifier_format = Vec::new();
        let mut txs_in_mempool_format = Vec::new();
        tokio::pin!(mempool_tx_stream);
        while txs_in_blockifier_format.len() < self.txs_chunk_size {
            match mempool_tx_stream.next().await {
                // TODO: the stream should be a enum and not tx, update once the BatcherClient trait
                // is ready
                Some(mempool_tx) => {
                    txs_in_mempool_format.push(mempool_tx.clone());
                    let blockifier_tx = BlockifierTransaction::AccountTransaction(
                        AccountTransaction::try_from(mempool_tx)?,
                    );
                    txs_in_blockifier_format.push(blockifier_tx);
                }
                // TODO: if None, should I give it another shot? should return error?
                None => {
                    break;
                }
            }
        }
        Ok(TxsChunk {
            mempool_format: txs_in_mempool_format,
            blockifier_format: txs_in_blockifier_format,
        })
    }
}

// TODO (yael 22/9/2024): implement this function for the next milestone
pub fn get_gas_prices() -> GasPrices {
    let one = NonZeroU128::new(1).unwrap();
    // TODO: L1 gas prices should be updated priodically and not necessarily on each block
    GasPrices::new(one, one, one, one, one, one)
}

#[derive(Debug)]
struct TxsChunk {
    mempool_format: Vec<Transaction>,
    blockifier_format: Vec<BlockifierTransaction>,
}

#[cfg_attr(test, derive(Clone))]
#[derive(Default, Debug, PartialEq)]
pub struct BlockExecutionArtifacts {
    // TODO(yael): what is the best id for mapping? tx_hash or tx_id? depends on the orchestrator
    // needs.
    pub execution_infos: HashMap<TransactionHash, TransactionExecutionInfo>,
    pub commitment_state_diff: CommitmentStateDiff,
    pub visited_segments_mapping: VisitedSegmentsMapping,
    pub bouncer_weights: BouncerWeights,
}
