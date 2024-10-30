use std::env;
use std::path::{Path, PathBuf};

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
    let servers = create_node_servers(config, &mut channels, components);

    (clients, servers)
}

/// Returns the absolute path from the project root.
pub fn get_absolute_path(relative_path: &str) -> PathBuf {
    Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("../..").join(relative_path)
}
