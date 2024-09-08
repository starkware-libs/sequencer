use blockifier::blockifier::block::{pre_process_block, BlockInfo, BlockNumberHashPair, GasPrices};
use blockifier::blockifier::config::TransactionExecutorConfig;
use blockifier::blockifier::transaction_executor::{TransactionExecutor, VisitedSegmentsMapping};
use blockifier::bouncer::{BouncerConfig, BouncerWeights};
use blockifier::context::{BlockContext, ChainInfo};
use blockifier::state::cached_state::{CachedState, CommitmentStateDiff};
use blockifier::state::errors::StateError;
use blockifier::state::state_api::StateReader;
use blockifier::versioned_constants::{VersionedConstants, VersionedConstantsOverrides};
use starknet_api::block::{BlockNumber, BlockTimestamp};
use starknet_api::core::ContractAddress;
use starknet_api::executable_transaction::Transaction;
use thiserror::Error;

pub struct BlockBuilder<S: StateReader> {
    pub executor: TransactionExecutor<S>,
}

#[derive(Debug, Error)]
pub enum BlockBuilderError {
    #[error(transparent)]
    BlockifierStateError(#[from] StateError),
    #[error(transparent)]
    BadTimestamp(#[from] std::num::TryFromIntError),
}

pub type BlockBuilderResult<T> = Result<T, BlockBuilderError>;

pub struct ExecutionConfig {
    pub chain_info: ChainInfo,
    pub execute_config: TransactionExecutorConfig,
    pub bouncer_config: BouncerConfig,
    pub sequencer_address: ContractAddress,
    pub use_kzg_da: bool,
    pub version_constants_overrides: VersionedConstantsOverrides,
}

impl<S: StateReader> BlockBuilder<S> {
    pub fn new(
        next_block_number: BlockNumber,
        state_reader: S,
        retrospective_block_hash: Option<BlockNumberHashPair>,
        execution_config: ExecutionConfig,
    ) -> BlockBuilderResult<Self> {
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
                execution_config.version_constants_overrides,
            ),
            execution_config.bouncer_config,
        );

        // TODO(Yael: 8/9/2024): Make sure to add a global_contract_cache to the state_reader.
        // TODO(Yael: 8/9/2024): align the state reader to the relevant block number.
        // TODO(Yael: 8/9/2024): Consider sharing the Storage trait from native blockifier.
        let mut state = CachedState::new(state_reader);

        pre_process_block(&mut state, retrospective_block_hash, next_block_number)?;

        Ok(BlockBuilder {
            executor: TransactionExecutor::new(
                state,
                block_context,
                execution_config.execute_config,
            ),
        })
    }

    /// Adds transactions to a block. Returns the block artifacts if the block is done.
    pub fn add_txs_and_stream(
        &self,
        _txs: &[Transaction],
    ) -> BlockBuilderResult<Option<(CommitmentStateDiff, VisitedSegmentsMapping, BouncerWeights)>>
    {
        todo!()
    }
}

fn get_gas_prices() -> GasPrices {
    // TODO: gas prices should be updated priodically and not necessarily on each block
    todo!()
}
