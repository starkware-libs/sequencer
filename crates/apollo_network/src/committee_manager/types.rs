use starknet_api::core::ContractAddress;
use starknet_api::staking::StakingWeight;

// TODO(noam.s): Unify with `apollo_staking::staking_manager::EpochId` once the cyclic dependency
// between apollo_network and apollo_staking is resolved.

// TODO(noam.s): Consider using `apollo_staking::committee_provider::Staker` directly.

/// Epoch identifier, matching the staking contract's epoch.
pub type EpochId = u64;

/// Committee identifier, derived as a hash of the sorted committee members's staker IDs.
pub use apollo_propeller::types::Channel as CommitteeId;

/// Staker identifier, same as the consensus ValidatorId.
pub type StakerId = ContractAddress;

/// A member of a committee.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommitteeMember {
    pub staker_id: StakerId,
    pub weight: StakingWeight,
}
