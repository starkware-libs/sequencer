pub mod config;
pub mod receiver;
pub mod sender;

use papyrus_network::gossipsub_impl::Topic;
use papyrus_network::network_manager::{BroadcastTopicChannels, NetworkManager};
use starknet_gateway_types::communication::SharedGatewayClient;

use crate::config::MempoolP2pConfig;
use crate::receiver::MempoolP2pRunner;
use crate::sender::MempoolP2pPropagator;

const MEMPOOL_TOPIC: &str = "starknet_mempool_transaction_propagation/0.1.0";

pub fn create_p2p_propagator_and_runner(
    mempool_p2p_config: MempoolP2pConfig,
    gateway_client: SharedGatewayClient,
) -> (MempoolP2pPropagator, MempoolP2pRunner) {
    let mut network_manager = NetworkManager::new(
        mempool_p2p_config.network_config,
        mempool_p2p_config.executable_version,
    );
    let BroadcastTopicChannels { broadcasted_messages_receiver, broadcast_topic_client } =
        network_manager
            .register_broadcast_topic(
                Topic::new(MEMPOOL_TOPIC),
                mempool_p2p_config.network_buffer_size,
            )
            .expect("Failed to register broadcast topic");
    let mempool_p2p_propagator = MempoolP2pPropagator::new(broadcast_topic_client.clone());
    let mempool_p2p_runner = MempoolP2pRunner::new(
        Some(network_manager),
        broadcasted_messages_receiver,
        broadcast_topic_client,
        gateway_client,
    );
    (mempool_p2p_propagator, mempool_p2p_runner)
}
