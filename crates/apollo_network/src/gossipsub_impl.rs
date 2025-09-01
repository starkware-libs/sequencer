//! GossipSub implementation for Apollo Network.
//!
//! This module provides a GossipSub-based message broadcasting system that enables
//! efficient and reliable message propagation across the Starknet network. It handles
//! message validation, peer management, and implements the necessary event bridging
//! to integrate with the broader networking system.
//!
//! ## Features
//!
//! - **Topic-based messaging**: Support for multiple topics with independent message flows
//! - **Message validation**: Built-in support for message validation and peer reporting
//! - **Reliable propagation**: Uses GossipSub's mesh networking for reliable delivery
//! - **Configurable topics**: Support for both identity-based (testing) and hash-based (production)
//!   topics
//!
//! ## Topic Types
//!
//! The module uses different topic types based on the build configuration:
//! - **Testing**: [`gossipsub::IdentTopic`] for deterministic behavior
//! - **Production**: [`gossipsub::Sha256Topic`] for security and privacy

use libp2p::gossipsub::TopicHash;
use libp2p::{gossipsub, PeerId};
use tracing::error;

use crate::mixed_behaviour::BridgedBehaviour;
use crate::{mixed_behaviour, Bytes};

#[cfg(test)]
pub type Topic = gossipsub::IdentTopic;
#[cfg(not(test))]
pub type Topic = gossipsub::Sha256Topic;

#[derive(Debug)]
pub enum ExternalEvent {
    #[allow(dead_code)]
    Received { originated_peer_id: PeerId, message: Bytes, topic_hash: TopicHash },
}

impl From<gossipsub::Event> for mixed_behaviour::Event {
    fn from(event: gossipsub::Event) -> Self {
        match event {
            gossipsub::Event::Message {
                message: gossipsub::Message { data, topic, source, .. },
                ..
            } => {
                let Some(originated_peer_id) = source else {
                    error!(
                        "Received a message from gossipsub without source even though we've \
                         configured it to reject such messages"
                    );
                    return mixed_behaviour::Event::ToOtherBehaviourEvent(
                        mixed_behaviour::ToOtherBehaviourEvent::NoOp,
                    );
                };
                mixed_behaviour::Event::ExternalEvent(mixed_behaviour::ExternalEvent::GossipSub(
                    ExternalEvent::Received {
                        originated_peer_id,
                        message: data,
                        topic_hash: topic,
                    },
                ))
            }
            _ => mixed_behaviour::Event::ToOtherBehaviourEvent(
                mixed_behaviour::ToOtherBehaviourEvent::NoOp,
            ),
        }
    }
}

impl BridgedBehaviour for gossipsub::Behaviour {
    fn on_other_behaviour_event(&mut self, _event: &mixed_behaviour::ToOtherBehaviourEvent) {}
}
