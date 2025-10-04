use apollo_network::network_manager::{
    BroadcastTopicChannels,
    BroadcastTopicClient,
    BroadcastTopicServer,
    NetworkManager,
    PropellerChannels,
    PropellerClient,
    PropellerMessageServer,
    SqmrClientSender,
    SqmrServerReceiver,
};
use apollo_network::NetworkConfig;

use crate::args::NetworkProtocol;
use crate::message_handling::{MessageReceiver, MessageSender, PropellerSender};
use crate::metrics::{create_network_metrics, TOPIC};

pub type TopicType = Vec<u8>;

pub const SQMR_PROTOCOL_NAME: &str = "/stress-test/1.0.0";

/// Creates peer configurations for Propeller protocol from bootstrap addresses
/// Each peer gets equal weight (1000) for simplicity
fn create_propeller_peers_from_bootstrap(bootstrap_addrs: &[String]) -> Vec<(libp2p::PeerId, u64)> {
    use std::str::FromStr;

    use libp2p::Multiaddr;

    bootstrap_addrs
        .iter()
        .filter_map(|addr_str| {
            // Parse the multiaddr and extract the peer ID
            match Multiaddr::from_str(addr_str.trim()) {
                Ok(multiaddr) => {
                    // Extract peer ID from the multiaddr (last component should be /p2p/<peer_id>)
                    for protocol in multiaddr.iter() {
                        if let libp2p::multiaddr::Protocol::P2p(peer_id) = protocol {
                            return Some((peer_id, 1000)); // Equal weight for all peers
                        }
                    }
                    tracing::warn!("No peer ID found in bootstrap address: {}", addr_str);
                    None
                }
                Err(e) => {
                    tracing::error!("Failed to parse bootstrap address '{}': {}", addr_str, e);
                    None
                }
            }
        })
        .collect()
}

/// Fallback function to create peer configurations when no bootstrap addresses are provided
/// This creates deterministic peer IDs from node indices for testing
fn create_propeller_peers_fallback(num_nodes: u64) -> Vec<(libp2p::PeerId, u64)> {
    use libp2p::identity::Keypair;

    (0..num_nodes)
        .map(|node_id| {
            // Create deterministic peer ID from node ID (same as stress test node)
            let mut private_key = [0u8; 32];
            private_key[0..8].copy_from_slice(&node_id.to_le_bytes());

            let keypair = Keypair::ed25519_from_bytes(private_key)
                .expect("Failed to create keypair from private key");
            let peer_id = keypair.public().to_peer_id();

            (peer_id, 1000) // Equal weight for all peers
        })
        .collect()
}

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
    Propeller {
        propeller_client: Option<PropellerClient<TopicType>>,
        propeller_messages_receiver: Option<PropellerMessageServer<TopicType>>,
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
            NetworkChannels::Propeller { propeller_client, propeller_messages_receiver: _ } => {
                MessageSender::Propeller(PropellerSender::new(
                    propeller_client.take().expect("propeller_client should be available"),
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
            NetworkChannels::Propeller { propeller_messages_receiver, propeller_client: _ } => {
                MessageReceiver::Propeller(
                    propeller_messages_receiver
                        .take()
                        .expect("propeller_messages_receiver should be available"),
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
    num_nodes: u64,
    _current_node_id: u64,
    bootstrap_addrs: &[String],
) -> (NetworkManager, NetworkChannels) {
    let network_metrics = create_network_metrics();
    let mut network_manager =
        NetworkManager::new(network_config.clone(), None, Some(network_metrics));

    let channels = match protocol {
        NetworkProtocol::Gossipsub => {
            let network_channels = network_manager
                .register_broadcast_topic::<TopicType>(TOPIC.clone(), buffer_size)
                .expect("Failed to register broadcast topic");
            let BroadcastTopicChannels { broadcasted_messages_receiver, broadcast_topic_client } =
                network_channels;

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

            NetworkChannels::Sqmr { sqmr_client: Some(sqmr_client), sqmr_server: Some(sqmr_server) }
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
        NetworkProtocol::Propeller => {
            // Create peer configurations from bootstrap addresses, or fallback to generated peers
            let peers = if !bootstrap_addrs.is_empty() {
                tracing::info!(
                    "Creating Propeller peers from {} bootstrap addresses",
                    bootstrap_addrs.len()
                );
                let peers = create_propeller_peers_from_bootstrap(bootstrap_addrs);
                tracing::info!(
                    "Successfully created {} Propeller peers from bootstrap addresses",
                    peers.len()
                );
                for (peer_id, weight) in &peers {
                    tracing::debug!("Propeller peer: {} (weight: {})", peer_id, weight);
                }
                peers
            } else {
                tracing::info!(
                    "No bootstrap addresses provided, using fallback peer generation for {} nodes",
                    num_nodes
                );
                create_propeller_peers_fallback(num_nodes)
            };

            // Register propeller channels
            let propeller_channels = network_manager
                .register_propeller_channels::<TopicType>(buffer_size, peers)
                .expect("Failed to register propeller channels");

            let PropellerChannels { propeller_messages_receiver, propeller_client } =
                propeller_channels;

            NetworkChannels::Propeller {
                propeller_client: Some(propeller_client),
                propeller_messages_receiver: Some(propeller_messages_receiver),
            }
        }
    };

    (network_manager, channels)
}
