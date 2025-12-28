//! Dynamic propeller tree computation logic.
//!
//! This module implements the core tree topology algorithm inspired by Solana's Turbine protocol.
//! The tree is computed dynamically for each shard using deterministic seeded randomization
//! based on the publisher and shard ID, making the network resilient to targeted attacks.

use libp2p::identity::PeerId;

use crate::types::PeerSetError;

pub type Stake = u64;

/// Propeller tree manager that computes tree topology on-demand for each publisher.
///
/// Propeller uses a distributed broadcast approach where:
/// - F = floor((N-1)/3) where N is total number of nodes
/// - F represents both max faulty nodes AND number of data shards
/// - Total shards = N-1 (F data shards + (N-1-F) coding shards)
/// - Message is "built" when F shards received (can reconstruct)
/// - Message is "received" when 2F shards received (guarantees gossip property)
/// - Each peer broadcasts received shards to all other peers (full mesh)
#[derive(Debug, Clone)]
pub struct PropellerTreeManager {
    /// All nodes in the cluster with their weights, sorted by (weight, peer_id) descending.
    nodes: Vec<(PeerId, Stake)>,
    /// This node's peer ID.
    local_peer_id: PeerId,
    /// This node's index in the nodes vector.
    local_peer_index: Option<usize>,
}

impl PropellerTreeManager {
    /// Create a new propeller tree manager.
    pub fn new(local_peer_id: PeerId) -> Self {
        Self { nodes: Vec::new(), local_peer_id, local_peer_index: None }
    }

    pub fn get_local_peer_id(&self) -> PeerId {
        self.local_peer_id
    }

    pub fn get_node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn get_nodes(&self) -> &[(PeerId, Stake)] {
        &self.nodes
    }

    fn calculate_f(&self) -> usize {
        let total_nodes = self.get_node_count();
        assert!(
            total_nodes >= 2,
            "Propeller protocol requires at least 2 nodes (got {})",
            total_nodes
        );
        // Ensure F is at least 1 for small networks (N=2,3)
        // Standard formula: F = floor((N-1)/3)
        std::cmp::max(1, (total_nodes - 1) / 3)
    }

    pub fn calculate_data_shards(&self) -> usize {
        self.calculate_f()
    }

    pub fn calculate_coding_shards(&self) -> usize {
        let total_nodes = self.get_node_count();
        let f = self.calculate_f();
        let total_shards = total_nodes - 1;
        total_shards - f
    }

    pub fn should_build(&self, shard_count: usize) -> bool {
        shard_count >= self.calculate_f()
    }

    pub fn should_receive(&self, shard_count: usize) -> bool {
        if self.get_node_count() <= 3 {
            return self.should_build(shard_count);
        }
        shard_count >= 2 * self.calculate_f()
    }

    /// Update the cluster nodes.
    /// Nodes are sorted by peer_id for deterministic behavior across all nodes.
    pub fn update_nodes(&mut self, mut nodes: Vec<(PeerId, Stake)>) -> Result<(), PeerSetError> {
        // Check that local peer is in the list before sorting
        if !nodes.iter().any(|(peer_id, _)| *peer_id == self.local_peer_id) {
            return Err(PeerSetError::LocalPeerNotInPeerWeights);
        }

        nodes.sort_by(|(a_peer_id, a_stake), (b_peer_id, b_stake)| {
            b_stake.cmp(a_stake).then_with(|| a_peer_id.cmp(b_peer_id))
        });

        let local_peer_index = nodes
            .iter()
            .position(|(peer_id, _)| *peer_id == self.local_peer_id)
            .expect("Local peer must be in nodes list (checked above)");

        self.nodes = nodes;
        self.local_peer_index = Some(local_peer_index);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_tree_manager() {
        let peer = PeerId::random();
        let manager = PropellerTreeManager::new(peer);
        assert_eq!(manager.get_local_peer_id(), peer);
        assert_eq!(manager.get_node_count(), 0);
    }

    #[test]
    fn test_update_nodes_and_f_calculation() {
        let peer1 = PeerId::random();
        let peer2 = PeerId::random();
        let peer3 = PeerId::random();
        let peer4 = PeerId::random();

        let mut manager = PropellerTreeManager::new(peer1);
        manager.update_nodes(vec![(peer1, 100), (peer2, 75), (peer3, 50), (peer4, 25)]).unwrap();

        assert_eq!(manager.get_node_count(), 4);
        // F = floor((4-1)/3) = 1
        assert_eq!(manager.calculate_data_shards(), 1);
        // Coding shards = (N-1) - F = 3 - 1 = 2
        assert_eq!(manager.calculate_coding_shards(), 2);
    }

    #[test]
    fn test_should_build_and_receive() {
        let peer1 = PeerId::random();
        let peer2 = PeerId::random();
        let peer3 = PeerId::random();
        let peer4 = PeerId::random();

        let mut manager = PropellerTreeManager::new(peer1);
        manager.update_nodes(vec![(peer1, 100), (peer2, 75), (peer3, 50), (peer4, 25)]).unwrap();

        // F = 1, so should build with 1 shard
        assert!(manager.should_build(1));
        assert!(!manager.should_build(0));

        // Should receive with 2F = 2 shards
        assert!(manager.should_receive(2));
        assert!(!manager.should_receive(1));
    }

    #[test]
    fn test_update_nodes_missing_local_peer() {
        let peer1 = PeerId::random();
        let peer2 = PeerId::random();

        let mut manager = PropellerTreeManager::new(peer1);
        let result = manager.update_nodes(vec![(peer2, 100)]);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), PeerSetError::LocalPeerNotInPeerWeights);
    }
}
