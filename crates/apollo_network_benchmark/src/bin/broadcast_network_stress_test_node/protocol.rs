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
use tracing::{error, info, trace};

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
        NetworkProtocol::ReveresedSqmr => {
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

            (
                MessageSender::ReveresedSqmr(ReveresedSqmrSender::new(sqmr_server)),
                MessageReceiver::ReveresedSqmr(sqmr_client),
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
    Sqmr(SqmrClientSender<TopicType, TopicType>),
    ReveresedSqmr(ReveresedSqmrSender),
}

/// Wrapper for ReveresedSqmr that maintains the last active query
pub struct ReveresedSqmrSender {
    server: SqmrServerReceiver<TopicType, TopicType>,
    active_query: Option<apollo_network::network_manager::ServerQueryManager<TopicType, TopicType>>,
}

impl ReveresedSqmrSender {
    pub fn new(server: SqmrServerReceiver<TopicType, TopicType>) -> Self {
        Self { server, active_query: None }
    }

    async fn collect_new_queries(&mut self) {
        // Non-blocking check for new queries, keeping only the last one
        while let Ok(query) =
            tokio::time::timeout(tokio::time::Duration::from_millis(1), self.server.next()).await
        {
            if let Some(query) = query {
                info!("ReveresedSqmr: Received new query, replacing previous query");
                self.active_query = Some(query);
            } else {
                break;
            }
        }
    }

    async fn broadcast_to_queries(&mut self, message: TopicType) {
        if let Some(query) = &mut self.active_query {
            match query.send_response(message).await {
                Ok(()) => {
                    trace!("ReveresedSqmr: Sent response to active query");
                }
                Err(e) => {
                    // Query failed, remove it
                    error!("ReveresedSqmr: Active query failed, removing it, error: {:?}", e);
                    self.active_query = None;
                }
            }
        }
    }
}

impl MessageSender {
    pub async fn send_message(&mut self, _peers: &[PeerId], message: TopicType) {
        match self {
            MessageSender::Gossipsub(client) => {
                client.broadcast_message(message).await.unwrap();
            }
            MessageSender::Sqmr(client) => {
                // Send query and properly handle the response manager to avoid session warnings
                match client.send_new_query(message).await {
                    Ok(mut response_manager) => {
                        // Consume the response manager to properly close the session
                        // This prevents the "finished with no messages" warning
                        tokio::spawn(async move {
                            while let Some(_response) = response_manager.next().await {
                                // Process any responses if they come, but don't block the sender
                            }
                        });
                    }
                    Err(e) => {
                        error!("Failed to send SQMR query: {:?}", e);
                    }
                }
            }
            MessageSender::ReveresedSqmr(sender) => {
                // Collect any new queries first
                sender.collect_new_queries().await;
                // Then broadcast the message to all active queries
                sender.broadcast_to_queries(message).await;
            }
        }
    }
}

// ================================
// MessageReceiver
// ================================

pub enum MessageReceiver {
    Gossipsub(BroadcastTopicServer<TopicType>),
    Sqmr(SqmrServerReceiver<TopicType, TopicType>),
    ReveresedSqmr(SqmrClientSender<TopicType, TopicType>),
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
                        let peer_id = message.1.originator_id.private_get_peer_id();
                        f(message.0.unwrap(), Some(peer_id));
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
            MessageReceiver::ReveresedSqmr(mut client) => loop {
                match client.send_new_query(vec![]).await {
                    Ok(mut response_manager) => loop {
                        let response_result = response_manager.next().await;
                        match response_result {
                            Some(Ok(response_data)) => {
                                f(response_data, None);
                            }
                            Some(Err(_)) => {
                                error!("ReveresedSqmr: Failed to parse response");
                                break;
                            }
                            None => {
                                error!("ReveresedSqmr: Response stream ended");
                                break;
                            }
                        }
                    },
                    Err(e) => {
                        error!(
                            "Failed to establish ReveresedSqmr connection, keeping client alive, \
                             error: {:?}",
                            e
                        );
                    }
                }
            },
        }
    }
}
