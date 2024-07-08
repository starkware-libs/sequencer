use std::env::args;
use std::process::exit;

use papyrus_config::validators::config_validate;
use papyrus_config::ConfigError;
use starknet_mempool_infra::trace_util::configure_tracing;
use starknet_mempool_node::communication::{create_node_channels, create_node_clients};
use starknet_mempool_node::components::create_components;
use starknet_mempool_node::config::MempoolNodeConfig;
use starknet_mempool_node::servers::{create_servers, run_server_components};
use tracing::{error, info};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    configure_tracing();

    let config = MempoolNodeConfig::load_and_process(args().collect());
    if let Err(ConfigError::CommandInput(clap_err)) = config {
        clap_err.exit();
    }

    let config = config?;
    if let Err(error) = config_validate(&config) {
        error!("{}", error);
        exit(1);
    }

    let channels = create_node_channels();
    let clients = create_node_clients(&config, &channels);
    let components = create_components(&config, &clients);
    let servers = create_servers(&config, channels, components);

    info!("Starting components!");
    run_server_components(&config, servers).await?;

    Ok(())
}
