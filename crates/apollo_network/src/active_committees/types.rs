/// Committee identifier, derived as a hash of the sorted committee members's staker IDs.
pub use apollo_propeller::types::Channel as CommitteeId;
use libp2p::PeerId;
pub use starknet_api::staking::EpochId;
use starknet_api::staking::StakingWeight;

// TODO(AndrewL): Consider moving `CommitteeMember` to `starknet_api::staking` (as
// `NetworkCommitteeMember`) to unify with `apollo_staking::committee_provider::Staker`. Currently
// blocked because `CommitteeMember` contains `PeerId` (libp2p), and `starknet_api` should not
// depend on libp2p.
/// A member of a committee.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommitteeMember {
    pub peer_id: PeerId,
    pub weight: StakingWeight,
}
