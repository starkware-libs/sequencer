// TODO(AndrewL): Remove this once the sender and receiver are used
#![allow(dead_code)]

use apollo_network::network_manager::{
    BroadcastTopicChannels,
    BroadcastTopicClient,
    BroadcastTopicClientTrait,
    BroadcastTopicServer,
    NetworkManager,
};
use apollo_network_benchmark::node_args::NetworkProtocol;
use futures::StreamExt;
use libp2p::gossipsub::{Sha256Topic, Topic};
use libp2p::PeerId;

// ================================
// Types and Constants
// ================================

lazy_static::lazy_static! {
    pub static ref TOPIC: Sha256Topic = Topic::new("stress_test_topic".to_string());
}

pub type TopicType = Vec<u8>;

/// Registers protocol channels on an existing network manager.
/// Returns a sender and receiver for the configured protocol.
pub fn register_protocol_channels(
    network_manager: &mut NetworkManager,
    buffer_size: usize,
    protocol: &NetworkProtocol,
) -> (MessageSender, MessageReceiver) {
    match protocol {
        NetworkProtocol::Gossipsub => {
            let channels = network_manager
                .register_broadcast_topic::<TopicType>(TOPIC.clone(), buffer_size)
                .expect("Failed to register broadcast topic");
            let BroadcastTopicChannels { broadcasted_messages_receiver, broadcast_topic_client } =
                channels;

            (
                MessageSender::Gossipsub(broadcast_topic_client),
                MessageReceiver::Gossipsub(broadcasted_messages_receiver),
            )
        }
    }
}

// ================================
// MessageSender
// ================================

/// Message sender abstraction for different protocols
pub enum MessageSender {
    Gossipsub(BroadcastTopicClient<TopicType>),
}

impl MessageSender {
    pub async fn send_message(&mut self, _peers: &[PeerId], message: TopicType) {
        match self {
            MessageSender::Gossipsub(client) => {
                client.broadcast_message(message).await.unwrap();
            }
        }
    }
}

// ================================
// MessageReceiver
// ================================

pub enum MessageReceiver {
    Gossipsub(BroadcastTopicServer<TopicType>),
}

impl MessageReceiver {
    pub async fn for_each<F>(self, mut f: F)
    where
        F: FnMut(TopicType, Option<PeerId>) + Copy,
    {
        match self {
            MessageReceiver::Gossipsub(receiver) => {
                receiver
                    .for_each(|message| async move {
                        let (payload_opt, meta) = message;
                        let peer_id = meta.originator_id.private_get_peer_id();
                        let payload =
                            payload_opt.expect("Broadcasted message should contain payload");
                        f(payload, Some(peer_id));
                    })
                    .await
            }
        }
    }
}
