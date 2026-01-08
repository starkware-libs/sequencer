// TODO(AndrewL): Remove this once the sender and receiver are used
#![allow(dead_code)]

use apollo_network::network_manager::{
    BroadcastTopicChannels,
    BroadcastTopicClient,
    BroadcastTopicClientTrait,
    BroadcastTopicServer,
    NetworkManager,
    SqmrClientSender,
    SqmrServerReceiver,
};
use apollo_network_benchmark::node_args::NetworkProtocol;
use futures::StreamExt;
use libp2p::gossipsub::{Sha256Topic, Topic};
use libp2p::PeerId;
use tracing::error;

// ================================
// Types and Constants
// ================================

lazy_static::lazy_static! {
    pub static ref TOPIC: Sha256Topic = Topic::new("stress_test_topic".to_string());
}

pub type TopicType = Vec<u8>;

pub const SQMR_PROTOCOL_NAME: &str = "/stress-test/1.0.0";

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
        NetworkProtocol::Sqmr => {
            let sqmr_client = network_manager
                .register_sqmr_protocol_client::<TopicType, TopicType>(
                    SQMR_PROTOCOL_NAME.to_string(),
                    buffer_size,
                );
            let sqmr_server = network_manager
                .register_sqmr_protocol_server::<TopicType, TopicType>(
                    SQMR_PROTOCOL_NAME.to_string(),
                    buffer_size,
                );

            (MessageSender::Sqmr(sqmr_client), MessageReceiver::Sqmr(sqmr_server))
        }
    }
}

// ================================
// MessageSender
// ================================

/// Message sender abstraction for different protocols
pub enum MessageSender {
    Gossipsub(BroadcastTopicClient<TopicType>),
    Sqmr(SqmrClientSender<TopicType, TopicType>),
}

impl MessageSender {
    pub async fn send_message(&mut self, _peers: &[PeerId], message: TopicType) {
        match self {
            MessageSender::Gossipsub(client) => {
                client.broadcast_message(message).await.unwrap();
            }
            MessageSender::Sqmr(client) => match client.send_new_query(message).await {
                Ok(mut response_manager) => {
                    tokio::spawn(async move {
                        while let Some(_response) = response_manager.next().await {}
                    });
                }
                Err(e) => {
                    error!("Failed to send SQMR query: {:?}", e);
                }
            },
        }
    }
}

// ================================
// MessageReceiver
// ================================

pub enum MessageReceiver {
    Gossipsub(BroadcastTopicServer<TopicType>),
    Sqmr(SqmrServerReceiver<TopicType, TopicType>),
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
            MessageReceiver::Sqmr(receiver) => {
                receiver
                    .for_each(|x| async move {
                        f(x.query().as_ref().unwrap().to_vec(), None);
                    })
                    .await
            }
        }
    }
}
