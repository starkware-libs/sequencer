use std::env::args;
use std::process::exit;

use papyrus_config::validators::config_validate;
use papyrus_config::ConfigError;
use starknet_mempool_infra::trace_util::configure_tracing;
use starknet_mempool_node::config::SequencerNodeConfig;
use starknet_mempool_node::servers::run_component_servers;
use starknet_mempool_node::utils::create_node_modules;
use tracing::{error, info};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    configure_tracing();

    let config = SequencerNodeConfig::load_and_process(args().collect());
    if let Err(ConfigError::CommandInput(clap_err)) = config {
        clap_err.exit();
    }
    info!("Finished loading configuration.");

    let config = config?;
    if let Err(error) = config_validate(&config) {
        error!("{}", error);
        exit(1);
    }
    info!("Finished validating configuration.");

    // Clients are currently unused, but should not be dropped.
    let (_clients, servers) = create_node_modules(&config);

    info!("Starting components!");
    run_component_servers(&config, servers).await?;

    Ok(())
}
