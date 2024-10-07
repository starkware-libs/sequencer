pub mod receiver;
pub mod sender;

use papyrus_network::gossipsub_impl::Topic;
use papyrus_network::network_manager::{BroadcastTopicChannels, NetworkManager};
use papyrus_network::NetworkConfig;

use crate::receiver::MempoolP2pReceiver;
use crate::sender::MempoolP2pSender;

pub fn create_p2p_sender_and_receiver(
    network_config: NetworkConfig,
    gateway_client: SharedGatewayClient,
    version: Option<String>,
    buffer_size: usize,
    topic: Topic,
) -> (MempoolP2pSender, MempoolP2pReceiver) {
    let mut network_manager = NetworkManager::new(network_config, version);
    let BroadcastTopicChannels { broadcasted_messages_receiver, broadcast_topic_client } =
        network_manager
            .register_broadcast_topic(topic, buffer_size)
            .expect("Failed to register broadcast topic");
    let mempool_p2p_sender = MempoolP2pSender::new(broadcast_topic_client.clone());
    let mempool_p2p_receiver = MempoolP2pReceiver::new(
        Some(network_manager),
        broadcasted_messages_receiver,
        broadcast_topic_client,
        gateway_client,
    );
    (mempool_p2p_sender, mempool_p2p_receiver)
}
