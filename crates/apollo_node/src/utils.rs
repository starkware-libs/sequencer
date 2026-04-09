use apollo_node_config::node_config::SequencerNodeConfig;
use metrics_exporter_prometheus::PrometheusHandle;
use tracing::info;

use crate::clients::{create_node_clients, SequencerNodeClients};
use crate::communication::{create_dynamic_config_channels, create_node_channels};
use crate::components::create_node_components;
use crate::servers::{create_node_servers, SequencerNodeServers};

pub async fn create_node_modules(
    config: &SequencerNodeConfig,
    prometheus_handle: Option<PrometheusHandle>,
    cli_args: Vec<String>,
) -> (SequencerNodeClients, SequencerNodeServers) {
    info!("Creating node modules.");

    let mut channels = create_node_channels(config);
    let mut dynamic_config_channels = create_dynamic_config_channels(config);
    let clients = create_node_clients(config, &mut channels, &mut dynamic_config_channels);
    let components = create_node_components(
        config,
        &clients,
        &mut dynamic_config_channels,
        prometheus_handle,
        cli_args,
    )
    .await;
    let servers = create_node_servers(config, &mut channels, components, &clients);

    (clients, servers)
}
