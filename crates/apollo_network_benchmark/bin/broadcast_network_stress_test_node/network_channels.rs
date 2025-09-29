use apollo_network::network_manager::{
    BroadcastTopicChannels,
    BroadcastTopicClient,
    BroadcastTopicServer,
    NetworkManager,
    SqmrClientSender,
    SqmrServerReceiver,
};
use apollo_network::NetworkConfig;

use crate::args::NetworkProtocol;
use crate::message_handling::{MessageReceiver, MessageSender};
use crate::metrics::{create_network_metrics, TOPIC};

pub type TopicType = Vec<u8>;

pub const SQMR_PROTOCOL_NAME: &str = "/stress-test/1.0.0";

/// Network communication channels for different protocols
pub enum NetworkChannels {
    Gossipsub {
        broadcast_topic_client: Option<BroadcastTopicClient<TopicType>>,
        broadcasted_messages_receiver: Option<BroadcastTopicServer<TopicType>>,
    },
    Sqmr {
        sqmr_client: Option<SqmrClientSender<TopicType, TopicType>>,
        sqmr_server: Option<SqmrServerReceiver<TopicType, TopicType>>,
    },
    ReveresedSqmr {
        sqmr_client: Option<SqmrClientSender<TopicType, TopicType>>,
        sqmr_server: Option<SqmrServerReceiver<TopicType, TopicType>>,
    },
}

impl NetworkChannels {
    pub fn take_sender(&mut self) -> MessageSender {
        match self {
            NetworkChannels::Gossipsub {
                broadcast_topic_client,
                broadcasted_messages_receiver: _,
            } => MessageSender::Gossipsub(
                broadcast_topic_client.take().expect("broadcast_topic_client should be available"),
            ),
            NetworkChannels::Sqmr { sqmr_client, sqmr_server: _ } => {
                MessageSender::Sqmr(sqmr_client.take().expect("sqmr_client should be available"))
            }
            NetworkChannels::ReveresedSqmr { sqmr_server, sqmr_client: _ } => {
                MessageSender::ReveresedSqmr(crate::message_handling::ReveresedSqmrSender::new(
                    sqmr_server.take().expect("sqmr_server should be available"),
                ))
            }
        }
    }

    pub fn take_receiver(&mut self) -> MessageReceiver {
        match self {
            NetworkChannels::Gossipsub {
                broadcasted_messages_receiver,
                broadcast_topic_client: _,
            } => MessageReceiver::Gossipsub(
                broadcasted_messages_receiver
                    .take()
                    .expect("broadcasted_messages_receiver should be available"),
            ),
            NetworkChannels::Sqmr { sqmr_server, sqmr_client: _ } => {
                MessageReceiver::Sqmr(sqmr_server.take().expect("sqmr_server should be available"))
            }
            NetworkChannels::ReveresedSqmr { sqmr_client, sqmr_server: _ } => {
                MessageReceiver::ReveresedSqmr(
                    sqmr_client.take().expect("sqmr_client should be available"),
                )
            }
        }
    }
}

/// Creates and sets up a network manager with protocol registration
#[allow(clippy::type_complexity)]
pub fn create_network_manager_with_channels(
    network_config: &NetworkConfig,
    buffer_size: usize,
    protocol: &NetworkProtocol,
) -> (NetworkManager, NetworkChannels) {
    let network_metrics = create_network_metrics();
    let mut network_manager =
        NetworkManager::new(network_config.clone(), None, Some(network_metrics));

    let channels = match protocol {
        NetworkProtocol::Gossipsub => {
            let network_channels = network_manager
                .register_broadcast_topic::<TopicType>(TOPIC.clone(), buffer_size)
                .expect("Failed to register broadcast topic");
            let BroadcastTopicChannels {
                broadcasted_messages_receiver,
                broadcast_topic_client,
            } = network_channels;

            NetworkChannels::Gossipsub {
                broadcast_topic_client: Some(broadcast_topic_client),
                broadcasted_messages_receiver: Some(broadcasted_messages_receiver),
            }
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

            NetworkChannels::Sqmr {
                sqmr_client: Some(sqmr_client),
                sqmr_server: Some(sqmr_server),
            }
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

            NetworkChannels::ReveresedSqmr {
                sqmr_client: Some(sqmr_client),
                sqmr_server: Some(sqmr_server),
            }
        }
    };

    (network_manager, channels)
}
