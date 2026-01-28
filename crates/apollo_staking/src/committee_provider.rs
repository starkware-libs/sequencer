use std::sync::Arc;

use apollo_consensus::types::Round;
use apollo_state_sync_types::communication::StateSyncClientError;
use async_trait::async_trait;
use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
use starknet_api::staking::StakingWeight;
use starknet_types_core::felt::Felt;
use thiserror::Error;

use crate::staking_contract::StakingContractError;

pub type Committee = Vec<Staker>;

#[cfg_attr(test, derive(Clone))]
#[derive(Debug, PartialEq, Eq, Hash)]
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
    StateSyncClientError(#[from] StateSyncClientError),
    #[error("Committee is empty.")]
    EmptyCommittee,
    #[error("Committee info unavailable for height {height}.")]
    InvalidHeight { height: BlockNumber },
    #[error(transparent)]
    StakingContractError(#[from] StakingContractError),
}

pub type CommitteeProviderResult<T> = Result<T, CommitteeProviderError>;

/// Trait for managing committee operations including fetching and selecting committee members
/// and proposers for consensus.
/// The committee is a subset of nodes (proposer and validators) that are selected to participate in
/// the consensus at a given epoch, responsible for proposing blocks and voting on them.
#[async_trait]
pub trait CommitteeProvider: Send + Sync {
    /// Returns a list of the committee members at the epoch of the given height.
    // TODO(Dafna): Consider including the total weight in the returned `Committee` type.
    async fn get_committee(&self, height: BlockNumber) -> CommitteeProviderResult<Arc<Committee>>;

    /// Returns the address of the proposer for the specified height and round.
    ///
    /// The proposer is deterministically selected for a given height and round, from the committee
    /// corresponding to the epoch associated with that height.
    async fn get_proposer(
        &self,
        height: BlockNumber,
        round: Round,
    ) -> CommitteeProviderResult<ContractAddress>;

    /// Returns the address of the actual proposer for the specified height and round.
    ///
    /// 1. Filters the committee to only include stakers eligible to propose (based on `can_propose`
    ///    field in StakerConfig).
    /// 2. Uses deterministic round-robin selection: `(height + round) % eligible_count`.
    async fn get_actual_proposer(
        &self,
        height: BlockNumber,
        round: Round,
    ) -> CommitteeProviderResult<ContractAddress>;
}
