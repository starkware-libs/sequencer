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

/// Topic type used for GossipSub messaging.
///
/// In test builds, uses [`gossipsub::IdentTopic`] for predictable behavior.
/// In production builds, uses [`gossipsub::Sha256Topic`] for enhanced security.
#[cfg(test)]
pub type Topic = gossipsub::IdentTopic;

/// Topic type used for GossipSub messaging.
///
/// In test builds, uses [`gossipsub::IdentTopic`] for predictable behavior.
/// In production builds, uses [`gossipsub::Sha256Topic`] for enhanced security.
#[cfg(not(test))]
pub type Topic = gossipsub::Sha256Topic;

/// External events emitted by the GossipSub behavior.
///
/// These events represent significant occurrences in the GossipSub protocol
/// that need to be handled by the broader networking system.
#[derive(Debug)]
pub enum ExternalEvent {
    /// A message was received from another peer.
    ///
    /// This event is triggered whenever a valid message is received through
    /// the GossipSub network for a subscribed topic.
    ///
    /// # Fields
    ///
    /// * `originated_peer_id` - The peer ID of the message originator
    /// * `message` - The raw message bytes
    /// * `topic_hash` - Hash identifying the topic this message belongs to
    ///
    /// # Message Handling
    ///
    /// Upon receiving this event, the network manager will:
    /// 1. Forward the message to registered topic subscribers
    /// 2. Provide metadata about the message originator
    /// 3. Enable validation and potential peer reporting
    #[allow(dead_code)]
    Received {
        /// The peer ID of the node that originally sent this message.
        originated_peer_id: PeerId,
        /// The raw message content as bytes.
        message: Bytes,
        /// The hash of the topic this message was sent on.
        topic_hash: TopicHash,
    },
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
