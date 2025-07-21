use std::sync::Arc;

use apollo_consensus::types::Round;
use apollo_state_sync_types::communication::{SharedStateSyncClient, StateSyncClientError};
use async_trait::async_trait;
use blockifier::context::BlockContext;
use blockifier::execution::errors::EntryPointExecutionError;
use blockifier::state::state_api::StateReader;
use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
use starknet_api::staking::StakingWeight;
use starknet_types_core::felt::Felt;
use thiserror::Error;

use crate::contract_types::RetdataDeserializationError;

pub type Committee = Vec<Staker>;

#[cfg_attr(test, derive(Clone))]
#[derive(Debug, PartialEq, Eq)]
pub struct Staker {
    // A contract address of the staker, to which rewards are sent.
    pub address: ContractAddress,
    // The staker's weight, which determines the staker's influence in the consensus (its voting
    // power).
    pub weight: StakingWeight,
    // The public key of the staker, used to verify the staker's identity.
    pub public_key: Felt,
}

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
    /// The proposer is deterministically selected for a given height and round, from the committee
    /// corresponding to the epoch associated with that height.
    async fn get_proposer<S: StateReader + Send>(
        &mut self,
        height: BlockNumber,
        round: Round,
        execution_context: ExecutionContext<S>,
    ) -> CommitteeProviderResult<ContractAddress>;
}
