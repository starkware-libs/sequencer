use std::collections::{HashMap, HashSet, VecDeque};

use libp2p::PeerId;

use super::types::StakerId;

/// Manages pending peer-to-staker mappings.
///
/// Tracks connections that have been validated but not yet established.
/// Maintains a bounded capacity with FIFO eviction when the bound is reached.
pub(crate) struct PendingMappings {
    /// Maps peer_id -> staker_id for pending connections.
    mappings: HashMap<PeerId, StakerId>,
    /// Maintains insertion order for FIFO eviction.
    /// Front is oldest, back is newest.
    insertion_order: VecDeque<PeerId>,
    /// Maximum number of pending mappings.
    capacity: usize,
}

#[allow(dead_code)]
impl PendingMappings {
    fn new(capacity: usize) -> Self {
        Self { mappings: HashMap::new(), insertion_order: VecDeque::new(), capacity }
    }

    /// Adds a pending connection mapping.
    /// If at capacity, evicts the oldest entry in FIFO order.
    fn add_pending_connection(&mut self, peer_id: PeerId, staker_id: StakerId) {
        // If peer already exists, remove it from the ordering first
        if self.mappings.contains_key(&peer_id) {
            self.insertion_order.retain(|p| p != &peer_id);
        } else if self.mappings.len() >= self.capacity {
            // Evict oldest entry if at capacity
            if let Some(oldest_peer_id) = self.insertion_order.pop_front() {
                self.mappings.remove(&oldest_peer_id);
            }
        }

        // Add the new mapping
        self.mappings.insert(peer_id, staker_id);
        self.insertion_order.push_back(peer_id);
    }

    /// If the peer exists, removes it from the mapping and returns the associated staker_id.
    fn pending_connection_established(&mut self, peer_id: &PeerId) -> Option<StakerId> {
        self.insertion_order.retain(|p| p != peer_id);
        self.mappings.remove(peer_id)
    }

    /// Removes a peer mapping.
    fn remove_pending_peer(&mut self, peer_id: &PeerId) {
        self.insertion_order.retain(|p| p != peer_id);
        self.mappings.remove(peer_id);
    }

    /// Removes all mappings whose value is the given staker_id.
    fn remove_pending_peers(&mut self, staker_id: &StakerId) {
        // collect peers whose mapped staker_id equals the input; fix comparison and clone keys
        let peers_to_remove: Vec<PeerId> = self
            .mappings
            .iter()
            .filter_map(|(k, v)| if v == staker_id { Some(*k) } else { None })
            .collect();

        // Remove from insertion_order in a single pass
        let peers_to_remove_set: HashSet<_> = peers_to_remove.iter().collect();
        self.insertion_order.retain(|p| !peers_to_remove_set.contains(p));

        // Remove from mappings
        for peer_id in peers_to_remove {
            self.mappings.remove(&peer_id);
        }
    }
}
