use starknet_api::core::ContractAddress;
use starknet_api::staking::StakingWeight;
use starknet_types_core::felt::Felt;

/// Epoch identifier, matching the staking contract's epoch.
pub type EpochId = u64;

/// Committee identifier, derived as a hash of the sorted committee members.
pub type CommitteeId = Felt;

/// Staker identifier, same as the consensus ValidatorId.
pub type StakerId = ContractAddress;

/// A member of a committee.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommitteeMember {
    pub staker_id: StakerId,
    pub weight: StakingWeight,
}
