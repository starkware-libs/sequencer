use std::sync::Arc;

use apollo_protobuf::consensus::Round;
use apollo_state_sync_types::communication::StateSyncClientError;
use async_trait::async_trait;
use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
use starknet_api::staking::StakingWeight;
use starknet_types_core::felt::Felt;
use thiserror::Error;

use crate::staking_contract::StakingContractError;

pub type StakerSet = Vec<Staker>;

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
    #[error("Missing epoch information for epoch {epoch_id}.")]
    MissingInformation { epoch_id: u64 },
    #[error(transparent)]
    StakingContractError(#[from] StakingContractError),
}

pub type CommitteeProviderResult<T> = Result<T, CommitteeProviderError>;

#[derive(Debug, Error)]
pub enum CommitteeError {
    #[error("Committee is empty.")]
    EmptyCommittee,
}

pub type CommitteeResult<T> = Result<T, CommitteeError>;

/// Trait for committee operations including proposer selection.
/// This trait is implemented by committee instances and provides synchronous methods
/// for determining proposers based on height and round.
#[cfg_attr(feature = "testing", mockall::automock)]
pub trait CommitteeTrait: Send + Sync {
    /// Returns a reference to the committee members.
    fn members(&self) -> &StakerSet;

    /// Returns the address of the proposer for the specified height and round.
    ///
    /// The proposer is deterministically selected for a given height and round using
    /// weighted random selection based on staker weights.
    fn get_proposer(&self, height: BlockNumber, round: Round) -> CommitteeResult<ContractAddress>;

    /// Returns the address of the actual proposer for the specified height and round.
    ///
    /// Uses deterministic round-robin selection from eligible proposers:
    /// `(height + round) % eligible_count`.
    fn get_actual_proposer(&self, height: BlockNumber, round: Round) -> ContractAddress;
}

/// Trait for managing committee operations including fetching committee instances.
/// The committee is a subset of nodes (proposer and validators) that are selected to participate in
/// the consensus at a given epoch, responsible for proposing blocks and voting on them.
#[async_trait]
#[cfg_attr(feature = "testing", mockall::automock)]
pub trait CommitteeProvider: Send + Sync {
    /// Returns a committee instance for the epoch of the given height.
    async fn get_committee(
        &self,
        height: BlockNumber,
    ) -> CommitteeProviderResult<Arc<dyn CommitteeTrait>>;
}
