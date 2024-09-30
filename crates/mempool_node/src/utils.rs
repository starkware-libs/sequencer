use crate::communication::{create_node_channels, create_node_clients, SequencerNodeClients};
use crate::components::create_components;
use crate::config::SequencerNodeConfig;
use crate::servers::{create_servers, SequencerNodeServers};

pub fn create_clients_servers_from_config(
    config: &SequencerNodeConfig,
) -> (SequencerNodeClients, SequencerNodeServers) {
    let mut channels = create_node_channels();
    let clients = create_node_clients(config, &mut channels);
    let components = create_components(config, &clients);
    let servers = create_servers(config, &mut channels, components);

    (clients, servers)
}
