use std::sync::Arc;

use apollo_consensus::types::Round;
use apollo_state_sync_types::communication::{SharedStateSyncClient, StateSyncClientError};
use async_trait::async_trait;
use blockifier::context::BlockContext;
use blockifier::execution::errors::EntryPointExecutionError;
use blockifier::state::state_api::StateReader;
use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
use thiserror::Error;

use crate::contract_types::{RetdataDeserializationError, Staker};

pub type Committee = Vec<Staker>;

#[derive(Debug, Error)]
pub enum CommitteeProviderError {
    #[error(transparent)]
    EntryPointExecutionError(#[from] EntryPointExecutionError),
    #[error(transparent)]
    RetdataDeserializationError(#[from] RetdataDeserializationError),
    #[error(transparent)]
    StateSyncClientError(#[from] StateSyncClientError),
    #[error("Committee is empty.")]
    EmptyCommittee,
}

pub type CommitteeProviderResult<T> = Result<T, CommitteeProviderError>;

#[cfg_attr(test, derive(Clone))]
pub struct ExecutionContext<S: StateReader> {
    pub state_reader: S,
    pub block_context: Arc<BlockContext>,
    pub state_sync_client: SharedStateSyncClient,
}

/// Trait for managing committee operations including fetching and selecting committee members
/// and proposers for consensus.
/// The committee is a subset of nodes (proposer and validators) that are selected to participate in
/// the consensus at a given epoch, responsible for proposing blocks and voting on them.
#[async_trait]
pub trait CommitteeProvider {
    /// Returns a list of the committee members at the given epoch.
    /// The state's most recent block should be provided in the execution_context.
    fn get_committee<S: StateReader>(
        &mut self,
        epoch: u64,
        execution_context: ExecutionContext<S>,
    ) -> CommitteeProviderResult<Arc<Committee>>;

    /// Returns the address of the proposer for the specified height and round.
    ///
    /// The proposer is chosen from the committee corresponding to the epoch of the given height.
    /// Selection is based on a deterministic random number derived from the height, round,
    /// and the hash of a past block — offset by `config.proposer_prediction_window`.
    async fn get_proposer<S: StateReader + Send>(
        &mut self,
        height: BlockNumber,
        round: Round,
        execution_context: ExecutionContext<S>,
    ) -> CommitteeProviderResult<ContractAddress>;
}
