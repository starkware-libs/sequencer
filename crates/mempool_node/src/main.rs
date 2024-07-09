use std::env::args;
use std::process::exit;

use papyrus_config::validators::config_validate;
use papyrus_config::ConfigError;
use starknet_mempool_node::communication::{create_node_channels, create_node_clients};
use starknet_mempool_node::components::create_components;
use starknet_mempool_node::config::MempoolNodeConfig;
use starknet_mempool_node::servers::{create_servers, run_server_components};
use tracing::metadata::LevelFilter;
use tracing::{error, info};
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, EnvFilter};

const DEFAULT_LEVEL: LevelFilter = LevelFilter::INFO;

fn configure_tracing() {
    let fmt_layer = fmt::layer().compact().with_target(false);
    let level_filter_layer =
        EnvFilter::builder().with_default_directive(DEFAULT_LEVEL.into()).from_env_lossy();

    // This sets a single subscriber to all of the threads. We may want to implement different
    // subscriber for some threads and use set_global_default instead of init.
    tracing_subscriber::registry().with(fmt_layer).with(level_filter_layer).init();
}

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
