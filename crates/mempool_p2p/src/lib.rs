pub mod receiver;
pub mod sender;

use std::collections::BTreeMap;

use papyrus_config::dumping::{append_sub_config_name, SerializeConfig};
use papyrus_config::{ParamPath, SerializedParam};
use papyrus_network::gossipsub_impl::Topic;
use papyrus_network::network_manager::{BroadcastTopicChannels, NetworkManager};
use papyrus_network::NetworkConfig;
use serde::{Deserialize, Serialize};
use starknet_gateway_types::communication::SharedGatewayClient;
use validator::Validate;

use crate::receiver::MempoolP2pRunner;
use crate::sender::MempoolP2pPropagator;

#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct MempoolP2pConfig {
    #[validate]
    pub network_config: NetworkConfig,
    node_version: Option<String>,
    buffer_size: usize,
}

impl SerializeConfig for MempoolP2pConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        append_sub_config_name(self.network_config.dump(), "network_config")
    }
}

pub fn create_p2p_propagator_and_runner(
    mempool_p2p_config: MempoolP2pConfig,
    gateway_client: SharedGatewayClient,
    topic: String,
) -> (Option<MempoolP2pPropagator>, Option<MempoolP2pRunner>) {
    let mut network_manager =
        NetworkManager::new(mempool_p2p_config.network_config, mempool_p2p_config.node_version);
    let BroadcastTopicChannels { broadcasted_messages_receiver, broadcast_topic_client } =
        network_manager
            .register_broadcast_topic(Topic::new(topic), mempool_p2p_config.buffer_size)
            .expect("Failed to register broadcast topic");
    let mempool_p2p_propagator = MempoolP2pPropagator::new(broadcast_topic_client.clone());
    let mempool_p2p_runner = MempoolP2pRunner::new(
        Some(network_manager),
        broadcasted_messages_receiver,
        broadcast_topic_client,
        gateway_client,
    );
    (Some(mempool_p2p_propagator), Some(mempool_p2p_runner))
}
