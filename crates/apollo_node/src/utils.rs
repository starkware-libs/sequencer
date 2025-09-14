use apollo_config::ConfigError;
use crate::config::node_config::SequencerNodeConfig;

use crate::clients::{create_node_clients, SequencerNodeClients};
use crate::communication::create_node_channels;
use crate::components::create_node_components;
use crate::servers::{create_node_servers, SequencerNodeServers};

pub async fn create_node_modules(
    config: &SequencerNodeConfig,
    cli_args: Vec<String>,
) -> (SequencerNodeClients, SequencerNodeServers) {
    info!("Creating node modules.");

    let mut channels = create_node_channels(config);
    let clients = create_node_clients(config, &mut channels);
    let components = create_node_components(config, &clients, cli_args).await;
    let servers = create_node_servers(config, &mut channels, components, &clients);

    (clients, servers)
}

pub fn load_and_validate_config(args: Vec<String>) -> Result<SequencerNodeConfig, ConfigError> {
    SequencerNodeConfig::load_and_process(args)
}
