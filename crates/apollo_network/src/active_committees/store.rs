#[cfg(test)]
#[path = "store_test.rs"]
mod store_test;

use std::collections::{BTreeSet, HashMap, VecDeque};

use libp2p::PeerId;
use sha2::{Digest, Sha256};
use starknet_api::staking::StakingWeight;

use super::types::{CommitteeId, CommitteeMember, EpochId};

fn sort_and_dedup_members(members: &mut [CommitteeMember]) -> Result<(), AddEpochError> {
    members.sort_by_key(|m| m.peer_id);

    for window in members.windows(2) {
        if window[0].peer_id == window[1].peer_id {
            return Err(AddEpochError::DuplicatePeerId(window[0].peer_id));
        }
    }
    Ok(())
}

fn compute_committee_id(sorted_members: &[CommitteeMember]) -> CommitteeId {
    let mut hasher = Sha256::new();
    for member in sorted_members {
        hasher.update(member.peer_id.to_bytes());
        hasher.update(member.weight.0.to_be_bytes());
    }
    let digest = hasher.finalize();

    // TODO(AndrewL): change this once propeller uses actual 256-bit hashes.
    let truncated = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]);
    CommitteeId(truncated)
}

#[derive(Debug, thiserror::Error)]
pub enum AddEpochError {
    #[error("Duplicate epoch ID {0}.")]
    DuplicateEpochId(EpochId),
    #[error("Duplicate peer ID {0} in committee members.")]
    DuplicatePeerId(PeerId),
}

/// Output of adding a new epoch to the store.
#[derive(Debug)]
pub struct AddEpochOutput {
    /// All peer IDs that are part of any active committee.
    pub allowed_peers: BTreeSet<PeerId>,
    /// If this epoch introduced a committee not previously tracked, contains the committee ID
    /// and its members as (peer_id, weight) pairs.
    pub new_committee: Option<(CommitteeId, Vec<(PeerId, StakingWeight)>)>,
    /// If adding this epoch caused an old committee to become inactive, contains its ID.
    pub removed_committee: Option<CommitteeId>,
}

/// Stores active epochs and derives committee and peer data from them.
///
/// Epochs are the single source of truth. A committee exists as long as at least one active epoch
/// references it. Peer allow-lists are derived from the union of all active committees.
#[derive(Debug)]
pub struct ActiveCommittees {
    capacity: usize,
    /// Insertion-ordered list of epoch IDs for FIFO eviction.
    epoch_order: VecDeque<EpochId>,
    /// Epoch data keyed by epoch ID.
    epochs: HashMap<EpochId, CommitteeId>,
    /// Number of active epochs referencing each committee ID.
    committee_ref_counts: HashMap<CommitteeId, u64>,
    /// The members for each tracked committee. Exists iff `committee_ref_counts` has the key.
    committee_data: HashMap<CommitteeId, Vec<CommitteeMember>>,
}

impl ActiveCommittees {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            epoch_order: VecDeque::new(),
            epochs: HashMap::new(),
            committee_ref_counts: HashMap::new(),
            committee_data: HashMap::new(),
        }
    }

    pub fn add_epoch(
        &mut self,
        epoch_id: EpochId,
        mut members: Vec<CommitteeMember>,
    ) -> Result<AddEpochOutput, AddEpochError> {
        if self.epochs.contains_key(&epoch_id) {
            return Err(AddEpochError::DuplicateEpochId(epoch_id));
        }

        sort_and_dedup_members(&mut members)?;
        let committee_id = compute_committee_id(&members);

        // Track the epoch.
        self.epoch_order.push_back(epoch_id);
        self.epochs.insert(epoch_id, committee_id);

        // Track the committee. A ref count of 0 (or absent) means this is a new committee.
        let count = self.committee_ref_counts.entry(committee_id).or_insert(0);
        let new_committee = if *count == 0 {
            let committee_peers = members.iter().map(|m| (m.peer_id, m.weight)).collect();
            self.committee_data.insert(committee_id, members);
            Some((committee_id, committee_peers))
        } else {
            None
        };
        *count += 1;

        // Evict the oldest epoch if over capacity.
        let removed_committee =
            if self.epoch_order.len() > self.capacity { self.evict_oldest_epoch() } else { None };

        let allowed_peers = self.compute_allowed_peers();

        Ok(AddEpochOutput { allowed_peers, new_committee, removed_committee })
    }

    /// Evicts the oldest epoch. If its committee has no remaining epochs, removes the committee
    /// and returns its ID.
    fn evict_oldest_epoch(&mut self) -> Option<CommitteeId> {
        let evicted_epoch_id = self.epoch_order.pop_front()?;

        let committee_id = self
            .epochs
            .remove(&evicted_epoch_id)
            .expect("epoch_order referenced an epoch not in epochs");

        let count = self
            .committee_ref_counts
            .get_mut(&committee_id)
            .expect("committee id in epochs references a committee not in committee_ref_counts");
        *count -= 1;

        if *count == 0 {
            self.committee_ref_counts.remove(&committee_id);
            self.committee_data.remove(&committee_id);
            Some(committee_id)
        } else {
            None
        }
    }

    fn compute_allowed_peers(&self) -> BTreeSet<PeerId> {
        self.committee_data.values().flat_map(|members| members.iter().map(|m| m.peer_id)).collect()
    }
}
