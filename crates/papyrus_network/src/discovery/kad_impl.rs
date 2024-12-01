use libp2p::kad;
use tracing::info;

use super::identify_impl::IdentifyToOtherBehaviourEvent;
use crate::mixed_behaviour::BridgedBehaviour;
use crate::{mixed_behaviour, peer_manager};

#[derive(Debug)]
pub enum KadToOtherBehaviourEvent {}

impl From<kad::Event> for mixed_behaviour::Event {
    fn from(_event: kad::Event) -> Self {
        mixed_behaviour::Event::ToOtherBehaviourEvent(mixed_behaviour::ToOtherBehaviourEvent::NoOp)
    }
}

impl<TStore: kad::store::RecordStore + Send + 'static> BridgedBehaviour for kad::Behaviour<TStore> {
    fn on_other_behaviour_event(&mut self, event: &mixed_behaviour::ToOtherBehaviourEvent) {
        match event {
            mixed_behaviour::ToOtherBehaviourEvent::Discovery(
                super::ToOtherBehaviourEvent::RequestKadQuery(peer_id),
            ) => {
                self.get_closest_peers(*peer_id);
            }
            mixed_behaviour::ToOtherBehaviourEvent::Identify(
                IdentifyToOtherBehaviourEvent::FoundListenAddresses { peer_id, listen_addresses },
            )
            | mixed_behaviour::ToOtherBehaviourEvent::Discovery(
                super::ToOtherBehaviourEvent::FoundListenAddresses { peer_id, listen_addresses },
            ) => {
                info!(
                    "Adding new listen addresses to routing table for peer {peer_id:?}: \
                     {listen_addresses:?}"
                );
                for address in listen_addresses {
                    self.add_address(peer_id, address.clone());
                }
            }
            mixed_behaviour::ToOtherBehaviourEvent::PeerManager(
                peer_manager::ToOtherBehaviourEvent::PeerBlacklisted { peer_id },
            ) => {
                self.remove_peer(peer_id);
            }
            _ => {}
        }
    }
}
