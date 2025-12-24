//! Dynamic propeller tree computation logic.
//!
//! This module implements the core tree topology algorithm inspired by Solana's Turbine protocol.
//! The tree is computed dynamically for each shard using deterministic seeded randomization
//! based on the publisher and shard ID, making the network resilient to targeted attacks.

use libp2p::identity::PeerId;

use crate::types::{PeerSetError, ShardIndex, TreeGenerationError};
use crate::{PropellerUnit, ShardValidationError};

pub type Stake = u64;

/// Propeller tree manager that computes tree topology on-demand for each publisher.
///
/// Propeller uses a distributed broadcast approach where:
/// - num_data_shards = floor((N-1)/3) where N is total number of nodes
/// - num_data_shards represents both max faulty nodes AND number of data shards
/// - Total shards = N-1 (num_data_shards data shards + (N-1-num_data_shards) coding shards)
/// - Message is "built" when num_data_shards shards received (can reconstruct)
/// - Message is "received" when 2*num_data_shards shards received (guarantees gossip property)
/// - Each peer broadcasts received shards to all other peers (full mesh)
#[derive(Debug, Clone)]
pub struct PropellerScheduleManager {
    /// All nodes in the channel with their stake, sorted by peer_id
    channel_nodes: Vec<(PeerId, Stake)>,
    /// This node's peer ID.
    local_peer_id: PeerId,
    /// This node's index in the nodes vector.
    #[allow(unused)] // TODO(AndrewL): remove this once we use it
    local_peer_index: usize,
    /// The number of data shards.
    num_data_shards: usize,
    /// The number of coding shards.
    num_coding_shards: usize,
}

impl PropellerScheduleManager {
    /// Create a new propeller tree manager.
    // TODO(AndrewL): What should I name the error type?
    pub fn new(
        local_peer_id: PeerId,
        mut nodes: Vec<(PeerId, Stake)>,
    ) -> Result<Self, PeerSetError> {
        // Check that local peer is in the list before sorting
        if !nodes.iter().any(|(peer_id, _)| *peer_id == local_peer_id) {
            return Err(PeerSetError::LocalPeerNotInPeerWeights);
        }

        nodes.sort();
        if nodes.windows(2).any(|window| window[0].0 == window[1].0) {
            return Err(PeerSetError::DuplicatePeerIds);
        }

        let local_peer_index = nodes
            .iter()
            .position(|(peer_id, _)| *peer_id == local_peer_id)
            .expect("Local peer must be in nodes list (checked above)");

        let total_nodes = nodes.len();
        // Ensure num_data_shards is at least 1 for small networks (N=2,3)
        // Standard formula: num_data_shards = floor((N-1)/3)
        let num_data_shards = std::cmp::max(1, (total_nodes - 1) / 3);
        let num_coding_shards = (total_nodes - 1).saturating_sub(num_data_shards);

        Ok(Self {
            channel_nodes: nodes,
            local_peer_id,
            local_peer_index,
            num_data_shards,
            num_coding_shards,
        })
    }

    pub fn get_local_peer_id(&self) -> PeerId {
        self.local_peer_id
    }

    pub fn get_node_count(&self) -> usize {
        self.channel_nodes.len()
    }

    pub fn get_nodes(&self) -> &[(PeerId, Stake)] {
        &self.channel_nodes
    }

    pub fn num_data_shards(&self) -> usize {
        self.num_data_shards
    }

    pub fn num_coding_shards(&self) -> usize {
        self.num_coding_shards
    }

    pub fn should_build(&self, shard_count: usize) -> bool {
        shard_count >= self.num_data_shards
    }

    pub fn should_receive(&self, shard_count: usize) -> bool {
        if self.get_node_count() <= 3 {
            return self.should_build(shard_count);
        }
        shard_count >= 2 * self.num_data_shards
    }

    pub fn get_peer_for_shard_id(
        &self,
        publisher: &PeerId,
        shard_index: ShardIndex,
    ) -> Result<PeerId, TreeGenerationError> {
        let original_shard_index = shard_index;
        let shard_index: usize = shard_index.0.try_into().unwrap();
        let publisher_index = self
            .channel_nodes
            .binary_search_by_key(&publisher, |(peer_id, _)| peer_id)
            .map_err(|_| TreeGenerationError::PublisherNotInChannel { publisher: *publisher })?;
        let index =
            if shard_index < publisher_index { shard_index } else { shard_index.saturating_add(1) };
        self.channel_nodes.get(index).map(|(peer, _)| *peer).ok_or({
            TreeGenerationError::ShardIndexOutOfBounds { shard_index: original_shard_index }
        })
    }

    pub fn validate_origin(
        &self,
        sender: PeerId,
        unit: &PropellerUnit,
    ) -> Result<(), ShardValidationError> {
        let local_peer_id = self.get_local_peer_id();
        assert_ne!(local_peer_id, sender, "sender cannot be the local peer id");

        let stated_publisher = unit.publisher();

        if stated_publisher == local_peer_id {
            return Err(ShardValidationError::ReceivedPublishedShard);
        }

        let stated_index = unit.index();
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
