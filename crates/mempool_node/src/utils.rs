use std::sync::Arc;

use starknet_mempool_p2p::sender::EmptyMempoolP2pSenderClient;

use crate::communication::{create_node_channels, create_node_clients, SequencerNodeClients};
use crate::components::create_node_components;
use crate::config::SequencerNodeConfig;
use crate::servers::{create_node_servers, SequencerNodeServers};

pub fn create_node_modules(
    config: &SequencerNodeConfig,
) -> (SequencerNodeClients, SequencerNodeServers) {
    let mut channels = create_node_channels();
    let clients = create_node_clients(config, &mut channels);
    let components = create_node_components(config, &clients);
    let servers = create_node_servers(
        config,
        &mut channels,
        components,
        Arc::new(EmptyMempoolP2pSenderClient),
    );

    (clients, servers)
}
