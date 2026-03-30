//! Dynamic propeller tree computation logic.
//!
//! This module implements the core tree topology algorithm inspired by Solana's Turbine protocol.

use libp2p::identity::PeerId;
use starknet_api::staking::StakingWeight;

use crate::types::{CommitteeSetupError, ScheduleError, UnitIndex};
use crate::UnitValidationError;

// TODO(AndrewL): add the concept of unit_owner when naming

/// Propeller tree manager that computes tree topology on-demand for each publisher.
///
/// Propeller uses a distributed broadcast approach where:
/// - num_data_shards = floor((N-1)/3) where N is total number of nodes
/// - num_data_shards represents both max faulty nodes AND number of data shards
/// - Total shards = N-1 (num_data_shards data shards + (N-1-num_data_shards) coding shards)
/// - Message is "built" when num_data_shards units received (can reconstruct)
/// - Message is "received" when 2*num_data_shards units received (guarantees gossip property)
/// - Each peer broadcasts received units to all other peers (full mesh)
#[derive(Debug, Clone)]
pub struct PropellerScheduleManager {
    /// All nodes in the committee with their stake, sorted by peer_id
    committee_nodes: Vec<(PeerId, StakingWeight)>,
    /// This node's peer ID.
    local_peer_id: PeerId,
    /// This node's index in the nodes vector.
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
        mut nodes: Vec<(PeerId, StakingWeight)>,
    ) -> Result<Self, CommitteeSetupError> {
        // Check that local peer is in the list before sorting
        if !nodes.iter().any(|(peer_id, _)| *peer_id == local_peer_id) {
            return Err(CommitteeSetupError::LocalPeerNotInCommittee);
        }

        nodes.sort();
        if nodes.windows(2).any(|window| window[0].0 == window[1].0) {
            return Err(CommitteeSetupError::DuplicatePeerIds);
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
            committee_nodes: nodes,
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
        self.committee_nodes.len()
    }

    pub fn get_nodes(&self) -> &[(PeerId, StakingWeight)] {
        &self.committee_nodes
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

    pub fn should_build(&self, unit_count: usize) -> bool {
        unit_count >= self.num_data_shards
    }

    pub fn should_receive(&self, unit_count: usize) -> bool {
        if self.get_node_count() <= 3 {
            return self.should_build(unit_count);
        }
        unit_count >= 2 * self.num_data_shards
    }

    /// Returns the peer responsible for broadcasting a specific unit.
    ///
    /// In the Propeller protocol, each unit is assigned to a specific peer (excluding the
<<<<<<< HEAD
    /// publisher). This method maps a shard index to its designated broadcaster.
=======
    /// publisher). This method maps a unit index to its designated broadcaster.
>>>>>>> 5665821e73 (apollo_l1_events: replace panic with retry in CatchUpper spawned task (#13328))
    ///
    /// # Arguments
    ///
    /// * `publisher` - The peer ID of the node that published the original message
<<<<<<< HEAD
<<<<<<< HEAD
    /// * `shard_index` - The index of the unit (0-based, ranges from 0 to total_shards-1)
<<<<<<< HEAD
    pub fn get_peer_for_shard_index(
=======
    pub fn get_peer_for_unit_index(
>>>>>>> 975f5e5e6a (apollo_l1_events: replace panic with retry in CatchUpper spawned task (#13328))
        &self,
        publisher: &PeerId,
        shard_index: UnitIndex,
=======
=======
>>>>>>> 020ac77342 (apollo_l1_events: replace panic with retry in CatchUpper spawned task (#13328))
    /// * `unit_index` - The index of the unit (0-based, ranges from 0 to total_shards-1)
=======
    /// * `unit_index` - The index of the unit (0-based, ranges from 0 to total_units-1)
>>>>>>> 956fe6c213 (apollo_l1_events: replace panic with retry in CatchUpper spawned task (#13328))
    pub fn get_peer_for_unit_index(
        &self,
        publisher: &PeerId,
        unit_index: UnitIndex,
>>>>>>> 5665821e73 (apollo_l1_events: replace panic with retry in CatchUpper spawned task (#13328))
    ) -> Result<PeerId, ScheduleError> {
        let original_unit_index = unit_index;
        let unit_index: usize = unit_index.0.try_into().expect("Failed converting u64 to usize");
        let publisher_index = self
            .committee_nodes
            .binary_search_by_key(&publisher, |(peer_id, _)| peer_id)
            .map_err(|_| ScheduleError::PublisherNotInCommittee { publisher: *publisher })?;
        let index =
            if unit_index < publisher_index { unit_index } else { unit_index.saturating_add(1) };
        self.committee_nodes
            .get(index)
            .map(|(peer, _)| *peer)
<<<<<<< HEAD
            .ok_or(ScheduleError::UnitIndexOutOfBounds { unit_index: original_shard_index })
=======
            .ok_or(ScheduleError::UnitIndexOutOfBounds { unit_index: original_unit_index })
>>>>>>> 5665821e73 (apollo_l1_events: replace panic with retry in CatchUpper spawned task (#13328))
    }

    /// Validates that a unit was received from the expected sender.
    ///
    /// Verifies that the sender is either the publisher (for direct units) or the designated
    /// broadcaster for this unit's index.
    ///
    /// # Arguments
    ///
    /// * `sender` - The peer ID that sent this unit
    /// * `unit` - The unit to validate
    pub fn validate_origin(
        &self,
        sender: PeerId,
        stated_publisher: PeerId,
        stated_index: UnitIndex,
    ) -> Result<(), UnitValidationError> {
        let local_peer_id = self.get_local_peer_id();
        if local_peer_id == sender {
            return Err(UnitValidationError::SelfSending);
        }

        if stated_publisher == local_peer_id {
            return Err(UnitValidationError::ReceivedSelfPublishedUnit);
        }

        let expected_broadcaster_for_index = self
<<<<<<< HEAD
            .get_peer_for_shard_index(&stated_publisher, stated_index)
=======
            .get_peer_for_unit_index(&stated_publisher, stated_index)
>>>>>>> 975f5e5e6a (apollo_l1_events: replace panic with retry in CatchUpper spawned task (#13328))
            .map_err(UnitValidationError::ScheduleManagerError)?;

        if expected_broadcaster_for_index == local_peer_id && sender == stated_publisher {
            // I received my unit from the publisher
            return Ok(());
        }
        if sender == expected_broadcaster_for_index {
            return Ok(());
        }
        // TODO(AndrewL): Make sure that the returned error allows for
        // distinguishing between the two cases.
        Err(UnitValidationError::UnexpectedSender {
            expected_sender: expected_broadcaster_for_index,
            unit_index: stated_index,
        })
    }

    /// Create the initial broadcast list for unit distribution.
    /// Returns a list of peer IDs for all peers except the publisher (local peer).
    pub fn make_broadcast_list(&self) -> Vec<PeerId> {
        let publisher = self.get_local_peer_id();
        self.committee_nodes
            .iter()
            .filter(|(peer_id, _)| *peer_id != publisher)
            .map(|(peer, _)| *peer)
            .collect()
    }

    /// Get the unit index that the local peer is responsible for when the given peer is the
    /// publisher.
    ///
    /// Returns an error if the local peer is the publisher (not in tree) or if the local peer
    /// is not found in the node list.
    pub fn get_my_unit_index_given_publisher(
        &self,
        publisher: &PeerId,
    ) -> Result<UnitIndex, ScheduleError> {
        if self.local_peer_id == *publisher {
            return Err(ScheduleError::LocalPeerIsPublisher);
        }

        let publisher_index = self
            .committee_nodes
            .binary_search_by_key(&publisher, |(peer_id, _)| peer_id)
            .map_err(|_| ScheduleError::PublisherNotInCommittee { publisher: *publisher })?;

<<<<<<< HEAD
        let unit_id = if self.local_peer_index < publisher_index {
=======
        let unit_index = if self.local_peer_index < publisher_index {
>>>>>>> be2415b786 (apollo_l1_events: replace panic with retry in CatchUpper spawned task (#13328))
            self.local_peer_index
        } else {
            self.local_peer_index - 1
        };

<<<<<<< HEAD
<<<<<<< HEAD
        Ok(UnitIndex(shard_id.try_into().unwrap()))
=======
        Ok(UnitIndex(unit_id.try_into().unwrap()))
>>>>>>> 5665821e73 (apollo_l1_events: replace panic with retry in CatchUpper spawned task (#13328))
=======
        Ok(UnitIndex(unit_id.try_into().unwrap()))
=======
        Ok(UnitIndex(unit_index.try_into().unwrap()))
>>>>>>> be2415b786 (apollo_l1_events: replace panic with retry in CatchUpper spawned task (#13328))
>>>>>>> fc0f890da4 (apollo_l1_events: replace panic with retry in CatchUpper spawned task (#13328))
    }
}
