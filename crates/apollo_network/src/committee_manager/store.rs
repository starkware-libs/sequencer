use std::collections::{HashMap, VecDeque};

use sha2::{Digest, Sha256};
use starknet_types_core::felt::Felt;

use super::types::{CommitteeId, CommitteeMember, EpochId, StakerId};

pub struct RegisterResult {
    pub new_committee_id: CommitteeId,
    pub committee_to_be_disconnected: Option<CommitteeId>,
    pub stakers_no_longer_in_any_committee: Vec<StakerId>,
}

#[derive(Debug)]
pub struct CommitteeStore {
    num_active_committees: usize,
    max_num_epochs: usize,
    epoch_order: VecDeque<EpochId>,
    epoch_to_committee: HashMap<EpochId, CommitteeId>,
    committee_data: HashMap<CommitteeId, Vec<CommitteeMember>>,
    committee_order: VecDeque<CommitteeId>,
    staker_ref_counts: HashMap<StakerId, u64>,
    last_committee_id: Option<CommitteeId>,
}

impl CommitteeStore {
    pub fn new(num_active_committees: usize, max_num_epochs: usize) -> Self {
        Self {
            num_active_committees,
            max_num_epochs,
            epoch_order: VecDeque::new(),
            epoch_to_committee: HashMap::new(),
            committee_data: HashMap::new(),
            committee_order: VecDeque::new(),
            staker_ref_counts: HashMap::new(),
            last_committee_id: None,
        }
    }
}

fn sort_members(members: &mut [CommitteeMember]) {
    members.sort_by_key(|m| (m.staker_id, m.weight));
}

fn compute_committee_id(sorted_members: &[CommitteeMember]) -> CommitteeId {
    let mut hasher = Sha256::new();
    for member in sorted_members {
        hasher.update(Felt::from(member.staker_id).to_bytes_be());
        hasher.update(member.weight.0.to_be_bytes());
    }
    let digest = hasher.finalize();

    let truncated = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]);
    CommitteeId(truncated)
}

impl CommitteeStore {
    pub fn get_committee(&self, epoch_id: &EpochId) -> Option<(CommitteeId, &[CommitteeMember])> {
        let committee_id = self.epoch_to_committee.get(epoch_id)?;
        let members = self.committee_data.get(committee_id)?;
        Some((*committee_id, members.as_slice()))
    }

    pub fn is_staker_in_any_active_committee(&self, staker_id: &StakerId) -> bool {
        self.staker_ref_counts.contains_key(staker_id)
    }

    pub fn register_epoch(
        &mut self,
        epoch_id: EpochId,
        mut members: Vec<CommitteeMember>,
    ) -> RegisterResult {
        sort_members(&mut members);

        let committee_id = self.resolve_committee_id(&members);
        self.last_committee_id = Some(committee_id);

        self.epoch_to_committee.insert(epoch_id, committee_id);
        self.epoch_order.push_back(epoch_id);

        if self.epoch_order.len() > self.max_num_epochs {
            if let Some(evicted_epoch_id) = self.epoch_order.pop_front() {
                self.epoch_to_committee.remove(&evicted_epoch_id);
            }
        }

        let (committee_to_be_disconnected, stakers_no_longer_in_any_committee) =
            if self.committee_order.len() > self.num_active_committees {
                let (evicted_id, evicted_stakers) = self.evict_oldest_committee();
                (Some(evicted_id), evicted_stakers)
            } else {
                (None, Vec::new())
            };

        RegisterResult {
            new_committee_id: committee_id,
            committee_to_be_disconnected,
            stakers_no_longer_in_any_committee,
        }
    }

    fn resolve_committee_id(&mut self, sorted_members: &[CommitteeMember]) -> CommitteeId {
        // Fast path: if the members are the same as last time, reuse the ID without hashing.
        if let Some(last_id) = self.last_committee_id {
            if self.committee_data.get(&last_id).is_some_and(|m| m == sorted_members) {
                self.committee_order.retain(|&id| id != last_id);
                self.committee_order.push_back(last_id);
                return last_id;
            }
        }

        let committee_id = compute_committee_id(sorted_members);

        if let Some(existing) = self.committee_data.get(&committee_id) {
            assert!(
                *existing == sorted_members,
                "CommitteeId hash collision: two different member sets produced the same \
                 CommitteeId({committee_id:?})"
            );
            self.committee_order.retain(|&id| id != committee_id);
            self.committee_order.push_back(committee_id);
        } else {
            for member in sorted_members {
                *self.staker_ref_counts.entry(member.staker_id).or_insert(0) += 1;
            }
            self.committee_data.insert(committee_id, sorted_members.to_vec());
            self.committee_order.push_back(committee_id);
        }

        committee_id
    }

    fn evict_oldest_committee(&mut self) -> (CommitteeId, Vec<StakerId>) {
        let evicted_committee_id = self
            .committee_order
            .pop_front()
            .expect("evict_oldest_committee called but committee_order is empty");

        let members = self
            .committee_data
            .remove(&evicted_committee_id)
            .expect("committee_order referenced a committee not in committee_data");

        // We don't need to eagerly remove epochs pointing to this committee from
        // `epoch_to_committee` because `get_committee` checks `committee_data` which we just
        // cleaned. `epoch_to_committee` is bounded by `max_num_epochs` anyway.

        let mut stakers_no_longer_in_any_committee = Vec::new();

        for member in &members {
            let staker_id = member.staker_id;
            if let Some(count) = self.staker_ref_counts.get_mut(&staker_id) {
                *count -= 1;
                if *count == 0 {
                    self.staker_ref_counts.remove(&staker_id);
                    stakers_no_longer_in_any_committee.push(staker_id);
                }
            }
        }

        (evicted_committee_id, stakers_no_longer_in_any_committee)
    }
}
