#[cfg(test)]
#[path = "store_test.rs"]
mod store_test;

use std::collections::{HashMap, VecDeque};

use libp2p::PeerId;

use super::types::{CommitteeId, CommitteeMember, EpochId, StakerId};

/// Errors that can occur when modifying the committee store.
#[derive(Debug, thiserror::Error)]
pub enum CommitteeStoreError {
    #[error("Staker {0:?} is not a member of any active committee.")]
    UnknownStaker(StakerId),
    #[error("Staker {0:?} already has a mapped peer.")]
    StakerAlreadyMapped(StakerId),
    #[error("Epoch {0} already exists in the store.")]
    EpochAlreadyExists(EpochId),
}

/// Committee state owned exclusively by the `CommitteeManagerBehaviour`.
///
/// The store enforces a configurable maximum number of active epochs (`num_active_epochs`).
/// When this limit is reached, the oldest epoch is evicted and any stakers that are no longer
/// referenced by any active epoch have their peer mappings removed. The evicted peer ids are
/// returned so the caller can disconnect them.
#[derive(Debug)]
pub struct CommitteeStore {
    /// Maximum number of tracked epochs. When exceeded, the oldest is evicted.
    num_active_epochs: usize,
    /// Epochs in insertion order (front = oldest).
    epoch_order: VecDeque<EpochId>,
    /// Epoch -> (CommitteeId, members).
    epoch_to_committee: HashMap<EpochId, (CommitteeId, Vec<CommitteeMember>)>,
    /// CommitteeId -> EpochId (reverse lookup).
    committee_id_to_epoch: HashMap<CommitteeId, EpochId>,
    /// StakerId -> number of active epochs containing this staker.
    staker_ref_counts: HashMap<StakerId, u64>,
    /// StakerId -> PeerId.
    staker_to_peer: HashMap<StakerId, PeerId>,
    /// PeerId -> StakerId (reverse mapping for disconnect cleanup).
    peer_to_staker: HashMap<PeerId, StakerId>,
}

impl CommitteeStore {
    pub fn new(num_active_epochs: usize) -> Self {
        Self {
            num_active_epochs,
            epoch_order: VecDeque::new(),
            epoch_to_committee: HashMap::new(),
            committee_id_to_epoch: HashMap::new(),
            staker_ref_counts: HashMap::new(),
            staker_to_peer: HashMap::new(),
            peer_to_staker: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Read API
// ---------------------------------------------------------------------------

impl CommitteeStore {
    /// Look up an epoch by its committee id.
    ///
    /// Returns the epoch id and the committee members, or `None` if the committee id is unknown.
    pub fn get_epoch(&self, committee_id: &CommitteeId) -> Option<(EpochId, &[CommitteeMember])> {
        let epoch_id = self.committee_id_to_epoch.get(committee_id)?;
        let (_, members) = self.epoch_to_committee.get(epoch_id)?;
        Some((*epoch_id, members.as_slice()))
    }

    /// Look up a committee by its epoch id.
    ///
    /// Returns the committee id and members, or `None` if the epoch id is unknown.
    pub fn get_committee(&self, epoch_id: &EpochId) -> Option<(CommitteeId, &[CommitteeMember])> {
        let (committee_id, members) = self.epoch_to_committee.get(epoch_id)?;
        Some((*committee_id, members.as_slice()))
    }
}

// TODO(noam): Make these methods visible only to the CommitteeManagerBehaviour.<
// ---------------------------------------------------------------------------
// Write API (used only by CommitteeManagerBehaviour)
// ---------------------------------------------------------------------------

impl CommitteeStore {
    /// Register a new epoch with its committee id and members.
    ///
    /// The `committee_id` should be pre-computed by the caller (CommitteeManagerBehaviour).
    /// Increments the reference count for each staker in the committee.
    ///
    /// If the number of tracked epochs has reached `num_active_epochs`, the oldest epoch is
    /// evicted first. Any stakers whose ref count drops to zero and who had a mapped peer are
    /// included in the returned `Vec<PeerId>` -- the caller should disconnect these peers.
    pub fn add_committee(
        &mut self,
        epoch_id: EpochId,
        committee_id: CommitteeId,
        members: Vec<CommitteeMember>,
    ) -> Result<Vec<PeerId>, CommitteeStoreError> {
        if self.epoch_to_committee.contains_key(&epoch_id) {
            return Err(CommitteeStoreError::EpochAlreadyExists(epoch_id));
        }

        // Increment ref counts for each staker.
        for member in &members {
            *self.staker_ref_counts.entry(member.staker_id).or_insert(0) += 1;
        }

        self.committee_id_to_epoch.insert(committee_id, epoch_id);
        self.epoch_to_committee.insert(epoch_id, (committee_id, members));
        self.epoch_order.push_back(epoch_id);

        // Evict the oldest epoch if we've reached the limit.
        let peers_to_disconnect = if self.epoch_order.len() > self.num_active_epochs {
            self.evict_oldest_epoch()
        } else {
            Vec::new()
        };

        Ok(peers_to_disconnect)
    }

    /// Check if a staker can be mapped to a peer (validation only, doesn't add the mapping).
    ///
    /// Returns an error if:
    /// - The staker is not a member of any active committee (`UnknownStaker`).
    /// - The staker already has a mapped peer (`StakerAlreadyMapped`).
    pub fn can_add_peer_for_staker(&self, staker_id: &StakerId) -> Result<(), CommitteeStoreError> {
        if !self.staker_ref_counts.contains_key(staker_id) {
            return Err(CommitteeStoreError::UnknownStaker(*staker_id));
        }
        if self.staker_to_peer.contains_key(staker_id) {
            return Err(CommitteeStoreError::StakerAlreadyMapped(*staker_id));
        }
        Ok(())
    }

    /// Map a staker to a peer.
    ///
    /// Fails if the staker is not in any active committee or if it already has a mapped peer.
    pub fn add_peer_for_staker(
        &mut self,
        staker_id: StakerId,
        peer_id: PeerId,
    ) -> Result<(), CommitteeStoreError> {
        if !self.staker_ref_counts.contains_key(&staker_id) {
            return Err(CommitteeStoreError::UnknownStaker(staker_id));
        }
        if self.staker_to_peer.contains_key(&staker_id) {
            return Err(CommitteeStoreError::StakerAlreadyMapped(staker_id));
        }

        self.staker_to_peer.insert(staker_id, peer_id);
        self.peer_to_staker.insert(peer_id, staker_id);

        Ok(())
    }

    /// Remove the staker-to-peer mapping for the given peer.
    ///
    /// Called when a peer disconnects. No-op if the peer has no mapping (e.g., the peer was never
    /// authenticated as a staker).
    pub fn remove_peer(&mut self, peer_id: &PeerId) {
        if let Some(staker_id) = self.peer_to_staker.remove(peer_id) {
            self.staker_to_peer.remove(&staker_id);
        }
    }

    /// Evict the oldest epoch. Decrements ref counts for its stakers and removes stakers with
    /// ref count 0. Returns the peer ids of any evicted stakers that had a mapped peer.
    fn evict_oldest_epoch(&mut self) -> Vec<PeerId> {
        let Some(oldest_epoch_id) = self.epoch_order.pop_front() else {
            return Vec::new();
        };

        let Some((committee_id, members)) = self.epoch_to_committee.remove(&oldest_epoch_id) else {
            return Vec::new();
        };

        self.committee_id_to_epoch.remove(&committee_id);

        let mut peers_to_disconnect = Vec::new();

        for member in &members {
            let staker_id = member.staker_id;
            if let Some(count) = self.staker_ref_counts.get_mut(&staker_id) {
                *count -= 1;
                if *count == 0 {
                    self.staker_ref_counts.remove(&staker_id);
                    // If this staker had a mapped peer, collect it for disconnection.
                    if let Some(peer_id) = self.staker_to_peer.remove(&staker_id) {
                        self.peer_to_staker.remove(&peer_id);
                        peers_to_disconnect.push(peer_id);
                    }
                }
            }
        }

        peers_to_disconnect
    }
}
