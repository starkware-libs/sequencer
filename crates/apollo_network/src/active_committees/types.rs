use libp2p::PeerId;
use starknet_api::staking::StakingWeight;

// TODO(noam.s): Unify with `apollo_staking::staking_manager::EpochId` once the cyclic dependency
// between apollo_network and apollo_staking is resolved.

/// Epoch identifier, matching the staking contract's epoch.
pub type EpochId = u64;

/// Committee identifier, derived as a hash of the sorted committee members's staker IDs.
pub use apollo_propeller::types::Channel as CommitteeId;

/// A member of a committee.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommitteeMember {
    pub peer_id: PeerId,
    pub weight: StakingWeight,
}
