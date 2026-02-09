use serde::{Deserialize, Serialize};

pub type PeerId = libp2p::PeerId;

// TODO(alonl): remove clone
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct BroadcastedMessageMetadata {
    pub originator_id: OpaquePeerId,
    pub encoded_message_length: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct OpaquePeerId(PeerId);

impl OpaquePeerId {
    /// This function shouldn't be used by anyone except for the apollo_network crate
    pub fn private_new(peer_id: PeerId) -> Self {
        Self(peer_id)
    }

    /// This function shouldn't be used by anyone except for the apollo_network crate
    pub fn private_get_peer_id(&self) -> PeerId {
        self.0
    }
}

// TODO(guyn): remove allow dead code once we use the duplicate vote report.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BadPeerReport {
    pub peer_id: OpaquePeerId,
    pub reason: String,
    pub penalty_card: PenaltyCard,
}

// TODO(guyn): need to decide how much misconduct score to add when getting each yellow card.
/// Represents the severity of the bad peer behavior.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PenaltyCard {
    /// Overtly malicious behavior.
    Red,
    /// Possibly sent malicious data on accident, will be considered malicious on repeat offenses.
    Yellow,
}
