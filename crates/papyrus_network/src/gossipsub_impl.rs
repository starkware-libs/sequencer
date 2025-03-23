use libp2p::floodsub::{self, FloodsubMessage};
use libp2p::gossipsub::TopicHash;
use libp2p::PeerId;

use crate::mixed_behaviour;
use crate::mixed_behaviour::BridgedBehaviour;
use crate::sqmr::Bytes;

#[derive(Debug)]
pub enum ExternalEvent {
    #[allow(dead_code)]
    Received { originated_peer_id: PeerId, message: Bytes, topic_hash: TopicHash },
}

impl From<floodsub::FloodsubEvent> for mixed_behaviour::Event {
    fn from(event: floodsub::FloodsubEvent) -> Self {
        match event {
            floodsub::FloodsubEvent::Message(FloodsubMessage {
                data,
                source: originated_peer_id,
                mut topics,
                ..
            }) => mixed_behaviour::Event::ExternalEvent(mixed_behaviour::ExternalEvent::GossipSub(
                ExternalEvent::Received {
                    originated_peer_id,
                    message: data.to_vec(),
                    topic_hash: TopicHash::from_raw(topics.pop().unwrap()),
                },
            )),
            _ => mixed_behaviour::Event::ToOtherBehaviourEvent(
                mixed_behaviour::ToOtherBehaviourEvent::NoOp,
            ),
        }
    }
}

impl BridgedBehaviour for floodsub::Floodsub {
    fn on_other_behaviour_event(&mut self, _event: &mixed_behaviour::ToOtherBehaviourEvent) {}
}
