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
    Received { originated_peer_id: PeerId, message: Bytes, topic_hash: TopicHash },
    Subscribed { peer_id: PeerId, topic_hash: TopicHash },
    Unsubscribed { peer_id: PeerId, topic_hash: TopicHash },
    GossipsubNotSupported { peer_id: PeerId },
    SlowPeer { peer_id: PeerId, failed_messages: gossipsub::FailedMessages },
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
            gossipsub::Event::Subscribed { peer_id, topic } => {
                mixed_behaviour::Event::ExternalEvent(mixed_behaviour::ExternalEvent::GossipSub(
                    ExternalEvent::Subscribed { peer_id, topic_hash: topic },
                ))
            }
            gossipsub::Event::Unsubscribed { peer_id, topic } => {
                mixed_behaviour::Event::ExternalEvent(mixed_behaviour::ExternalEvent::GossipSub(
                    ExternalEvent::Unsubscribed { peer_id, topic_hash: topic },
                ))
            }
            gossipsub::Event::GossipsubNotSupported { peer_id } => {
                mixed_behaviour::Event::ExternalEvent(mixed_behaviour::ExternalEvent::GossipSub(
                    ExternalEvent::GossipsubNotSupported { peer_id },
                ))
            }
            gossipsub::Event::SlowPeer { peer_id, failed_messages } => {
                mixed_behaviour::Event::ExternalEvent(mixed_behaviour::ExternalEvent::GossipSub(
                    ExternalEvent::SlowPeer { peer_id, failed_messages },
                ))
            }
        }
    }
}

impl BridgedBehaviour for gossipsub::Behaviour {
    fn on_other_behaviour_event(&mut self, _event: &mixed_behaviour::ToOtherBehaviourEvent) {}
}
