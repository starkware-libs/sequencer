//! Dynamic propeller tree computation logic.
//!
//! This module implements the core tree topology algorithm inspired by Solana's Turbine protocol.
//! The tree is computed dynamically for each shard using deterministic seeded randomization
//! based on the publisher and shard ID, making the network resilient to targeted attacks.

use libp2p::identity::PeerId;

use crate::types::{PeerSetError, ShardIndex, TreeGenerationError};
use crate::ShardValidationError;

pub type Stake = u64;

// TODO(AndrewL): add the concept of shard_owner when naming

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
    // TODO(AndrewL): What should I name the error type?
    pub fn new(
        local_peer_id: PeerId,
        mut nodes: Vec<(PeerId, Stake)>,
    ) -> Result<Self, PeerSetError> {
        // Check that local peer is in the list before sorting
        if !nodes.iter().any(|(peer_id, _)| *peer_id == local_peer_id) {
            return Err(PeerSetError::LocalPeerNotInChannel);
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
        // we reduce N by 1 because we exclude the publisher
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

    pub fn num_shards(&self) -> usize {
        self.num_data_shards + self.num_coding_shards
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

    /// Returns the peer responsible for broadcasting a specific shard.
    ///
    /// In the Propeller protocol, each shard is assigned to a specific peer (excluding the
    /// publisher). This method maps a shard index to its designated broadcaster.
    ///
    /// # Arguments
    ///
    /// * `publisher` - The peer ID of the node that published the original message
    /// * `shard_index` - The index of the shard (0-based, ranges from 0 to total_shards-1)
    pub fn get_peer_for_shard_index(
        &self,
        publisher: &PeerId,
        shard_index: ShardIndex,
    ) -> Result<PeerId, TreeGenerationError> {
        let original_shard_index = shard_index;
        let shard_index: usize = shard_index.0.try_into().expect("Failed converting u64 to usize");
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

    /// Validates that a shard unit was received from the expected sender.
    ///
    /// Verifies that the sender is either the publisher (for direct shards) or the designated
    /// broadcaster for this shard index.
    ///
    /// # Arguments
    ///
    /// * `sender` - The peer ID that sent this shard unit
    /// * `unit` - The shard unit to validate
    pub fn validate_origin(
        &self,
        sender: PeerId,
        stated_publisher: PeerId,
        stated_index: ShardIndex,
    ) -> Result<(), ShardValidationError> {
        let local_peer_id = self.get_local_peer_id();
        if local_peer_id == sender {
            return Err(ShardValidationError::SelfSending);
        }

        if stated_publisher == local_peer_id {
            return Err(ShardValidationError::ReceivedSelfPublishedShard);
        }

        let expected_broadcaster_for_index = self
            .get_peer_for_shard_index(&stated_publisher, stated_index)
            .map_err(ShardValidationError::ScheduleManagerError)?;

        if expected_broadcaster_for_index == local_peer_id && sender == stated_publisher {
            // I received my shard from the publisher
            return Ok(());
        }
        if sender == expected_broadcaster_for_index {
            return Ok(());
        }
        // TODO(AndrewL): Make sure that the returned error allows for
        // distinguishing between the two cases.
        Err(ShardValidationError::UnexpectedSender {
            expected_sender: expected_broadcaster_for_index,
            shard_index: stated_index,
        })
    }

    /// Create the initial broadcast list for message sharding.
    /// Returns a list of (peer_id, shard_index) pairs for all peers except the publisher.
    pub fn make_broadcast_list(&self) -> Vec<PeerId> {
        let publisher = self.get_local_peer_id();
        let mut broadcast_list = Vec::with_capacity(self.num_shards());
        for (peer, _) in self.channel_nodes.iter().filter(|(peer_id, _)| *peer_id != publisher) {
            broadcast_list.push(*peer);
        }
        broadcast_list
    }

    /// Get the shard ID that the local peer is responsible for when the given peer is the
    /// publisher.
    ///
    /// Returns an error if the local peer is the publisher (not in tree) or if the local peer
    /// is not found in the node list.
    pub fn get_my_shard_index(
        &self,
        publisher: &PeerId,
    ) -> Result<ShardIndex, TreeGenerationError> {
        if self.local_peer_id == *publisher {
            return Err(TreeGenerationError::LocalPeerIsPublisher);
        }

        let publisher_index = self
            .channel_nodes
            .binary_search_by_key(&publisher, |(peer_id, _)| peer_id)
            .map_err(|_| TreeGenerationError::PublisherNotInChannel { publisher: *publisher })?;

        let shard_id = if self.local_peer_index < publisher_index {
            self.local_peer_index
        } else {
            self.local_peer_index - 1
        };

        Ok(ShardIndex(shard_id.try_into().unwrap()))
    }
}
