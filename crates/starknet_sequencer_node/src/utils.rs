use metrics_exporter_prometheus::PrometheusBuilder;

use crate::clients::{create_node_clients, SequencerNodeClients};
use crate::communication::create_node_channels;
use crate::components::create_node_components;
use crate::config::node_config::SequencerNodeConfig;
use crate::servers::{create_node_servers, SequencerNodeServers};

pub fn create_node_modules(
    config: &SequencerNodeConfig,
) -> (SequencerNodeClients, SequencerNodeServers) {
    let mut channels = create_node_channels();
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    let clients = create_node_clients(config, &mut channels);
    let components = create_node_components(config, &clients, recorder.handle());
    let servers = create_node_servers(config, &mut channels, components, &clients);

    (clients, servers)
}
