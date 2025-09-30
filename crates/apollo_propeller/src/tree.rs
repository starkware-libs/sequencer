//! Dynamic propeller tree computation logic.
//!
//! This module implements the core tree topology algorithm inspired by Solana's Turbine protocol.
//! The tree is computed dynamically for each shard using deterministic seeded randomization
//! based on the publisher and shard ID, making the network resilient to targeted attacks.

use libp2p::identity::PeerId;

use crate::types::{PeerSetError, ShardIndex, TreeGenerationError};
use crate::{PropellerMessage, ShardValidationError};

type Stake = u64;

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
pub(crate) struct PropellerTreeManager {
    /// All nodes in the cluster with their weights, sorted by (weight, peer_id) descending.
    nodes: Vec<(PeerId, Stake)>,
    /// This node's peer ID.
    local_peer_id: PeerId,
    /// This node's index in the nodes vector.
    local_peer_index: Option<usize>,
}

impl PropellerTreeManager {
    /// Create a new propeller tree manager.
    pub(crate) fn new(local_peer_id: PeerId) -> Self {
        Self { nodes: Vec::new(), local_peer_id, local_peer_index: None }
    }

    pub(crate) fn get_local_peer_id(&self) -> PeerId {
        self.local_peer_id
    }

    pub(crate) fn get_node_count(&self) -> usize {
        self.nodes.len()
    }

    pub(crate) fn get_nodes(&self) -> &[(PeerId, Stake)] {
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

    pub(crate) fn calculate_data_shards(&self) -> usize {
        self.calculate_f()
    }

    pub(crate) fn calculate_coding_shards(&self) -> usize {
        let total_nodes = self.get_node_count();
        let f = self.calculate_f();
        let total_shards = total_nodes - 1;
        total_shards - f
    }

    pub(crate) fn should_build(&self, shard_count: usize) -> bool {
        shard_count >= self.calculate_f()
    }

    pub(crate) fn should_receive(&self, shard_count: usize) -> bool {
        if self.get_node_count() <= 3 {
            return self.should_build(shard_count);
        }
        shard_count >= 2 * self.calculate_f()
    }

    /// Update the cluster nodes.
    /// Nodes are sorted by peer_id for deterministic behavior across all nodes.
    pub(crate) fn update_nodes(
        &mut self,
        mut nodes: Vec<(PeerId, Stake)>,
    ) -> Result<(), PeerSetError> {
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

    /// Get the peer that should receive a specific shard ID for a given publisher.
    ///
    /// The tree is computed on-demand by excluding the publisher from the node list.
    /// Returns `None` if the shard ID is out of bounds or the publisher is not found.
    pub(crate) fn get_peer_for_shard_id(
        &self,
        publisher: &PeerId,
        shard_index: ShardIndex,
    ) -> Result<PeerId, TreeGenerationError> {
        let publisher_node_index = self
            .nodes
            .iter()
            .position(|(peer_id, _)| *peer_id == *publisher)
            .ok_or(TreeGenerationError::PublisherNotFound { publisher: *publisher })?;

        let i: usize = shard_index.0.try_into().unwrap();

        if i >= self.nodes.len() - 1 {
            return Err(TreeGenerationError::ShardIndexOutOfBounds { shard_index });
        }

        let actual_index = if i < publisher_node_index { i } else { i + 1 };

        Ok(self.nodes[actual_index].0)
    }

    /// Get the shard ID that the local peer is responsible for when the given peer is the
    /// publisher.
    ///
    /// Returns `None` if the local peer is the publisher (not in tree) or if the local peer
    /// is not found in the node list.
    pub(crate) fn get_my_shard_index(
        &self,
        publisher: &PeerId,
    ) -> Result<ShardIndex, TreeGenerationError> {
        if self.local_peer_id == *publisher {
            return Err(TreeGenerationError::LocalPeerIsPublisher);
        }

        let publisher_node_index = self
            .nodes
            .iter()
            .position(|(peer_id, _)| *peer_id == *publisher)
            .ok_or(TreeGenerationError::PublisherNotFound { publisher: *publisher })?;

        let local_node_index =
            self.local_peer_index.ok_or(TreeGenerationError::LocalPeerNotInPeerWeights)?;

        let shard_id = if local_node_index < publisher_node_index {
            local_node_index
        } else {
            local_node_index - 1
        };

        Ok(ShardIndex(shard_id.try_into().unwrap()))
    }

    pub(crate) fn validate_origin(
        &self,
        sender: PeerId,
        message: &PropellerMessage,
    ) -> Result<(), ShardValidationError> {
        let local_peer_id = self.get_local_peer_id();
        assert_ne!(local_peer_id, sender, "sender cannot be the local peer id");

        let stated_publisher = message.publisher();

        if stated_publisher == local_peer_id {
            return Err(ShardValidationError::ReceivedPublishedShard);
        }

        let stated_index = message.index();
        let expected_broadcaster_for_index = self
            .get_peer_for_shard_id(&stated_publisher, stated_index)
            .map_err(ShardValidationError::TreeError)?;

        if expected_broadcaster_for_index == local_peer_id {
            if sender == stated_publisher {
                return Ok(());
            }
        } else if sender == expected_broadcaster_for_index {
            return Ok(());
        }
        Err(ShardValidationError::UnexpectedSender {
            expected_sender: expected_broadcaster_for_index,
            shard_index: stated_index,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_peer_id(_id: u8) -> PeerId {
        PeerId::random()
    }

    #[test]
    fn test_tree_topology_with_publisher_excluded() {
        let peer1 = create_test_peer_id(1);
        let peer2 = create_test_peer_id(2);
        let peer3 = create_test_peer_id(3);
        let peer4 = create_test_peer_id(4);

        let publisher = peer2; // Publisher in the middle

        let mut manager = PropellerTreeManager::new(peer1);
        manager.update_nodes(vec![(peer1, 100), (peer2, 75), (peer3, 50), (peer4, 25)]).unwrap();

        // Tree should be [peer1, peer3, peer4] (excluding publisher peer2)
        assert_eq!(manager.get_peer_for_shard_id(&publisher, ShardIndex(0)).unwrap(), peer1);
        assert_eq!(manager.get_peer_for_shard_id(&publisher, ShardIndex(1)).unwrap(), peer3);
        assert_eq!(manager.get_peer_for_shard_id(&publisher, ShardIndex(2)).unwrap(), peer4);
        assert_eq!(
            manager.get_peer_for_shard_id(&publisher, ShardIndex(3)).unwrap_err(),
            TreeGenerationError::ShardIndexOutOfBounds { shard_index: ShardIndex(3) }
        );
    }
}
