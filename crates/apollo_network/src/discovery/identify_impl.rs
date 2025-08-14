use libp2p::{identify, Multiaddr, PeerId};

use crate::mixed_behaviour;
use crate::mixed_behaviour::BridgedBehaviour;
use crate::utils::is_localhost;

pub const IDENTIFY_PROTOCOL_VERSION: &str = "/staknet/identify/0.1.0-rc.0";

#[derive(Debug)]
pub enum IdentifyToOtherBehaviourEvent {
    FoundListenAddresses { peer_id: PeerId, listen_addresses: Vec<Multiaddr> },
}

impl From<identify::Event> for mixed_behaviour::Event {
    fn from(event: identify::Event) -> Self {
        match event {
            identify::Event::Received { peer_id, info, connection_id: _ } => {
                // Filtering out localhost since it might collide with our own listen address if we
                // use the same port.
                // No need to filter out in discovery since there the address comes from the
                // config, so if the user specified it they should make sure it doesn't collide
                // with our own address
                let listen_addresses = info
                    .listen_addrs
                    .into_iter()
                    .filter(|address| !is_localhost(address))
                    .collect();
                mixed_behaviour::Event::ToOtherBehaviourEvent(
                    mixed_behaviour::ToOtherBehaviourEvent::Identify(
                        IdentifyToOtherBehaviourEvent::FoundListenAddresses {
                            peer_id,
                            listen_addresses,
                        },
                    ),
                )
            }
            // TODO(shahak): Consider logging error events.
            _ => mixed_behaviour::Event::ToOtherBehaviourEvent(
                mixed_behaviour::ToOtherBehaviourEvent::NoOp,
            ),
        }
    }
}

impl BridgedBehaviour for identify::Behaviour {
    fn on_other_behaviour_event(&mut self, _event: &mixed_behaviour::ToOtherBehaviourEvent) {}
}
