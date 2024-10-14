use libp2p::PeerId;
use serde::{Deserialize, Serialize};

// TODO(alonl): remove clone
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Hash)]
pub struct BroadcastedMessageMetadata {
    pub originator_id: OpaquePeerId,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Hash)]
pub struct OpaquePeerId(PeerId);

impl OpaquePeerId {
    /// This function shouldn't be used by anyone except for the papyrus_network crate
    pub fn private_new(peer_id: PeerId) -> Self {
        Self(peer_id)
    }

    /// This function shouldn't be used by anyone except for the papyrus_network crate
    pub fn private_get_peer_id(&self) -> PeerId {
        self.0
    }
}
