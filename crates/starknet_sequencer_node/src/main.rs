use std::env::args;
use std::process::exit;

use papyrus_config::validators::config_validate;
use papyrus_config::ConfigError;
use starknet_sequencer_infra::trace_util::configure_tracing;
use starknet_sequencer_node::config::node_config::SequencerNodeConfig;
use starknet_sequencer_node::servers::run_component_servers;
use starknet_sequencer_node::utils::create_node_modules;
use tracing::{error, info};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    configure_tracing();

    let config = SequencerNodeConfig::load_and_process(args().collect());
    if let Err(ConfigError::CommandInput(clap_err)) = config {
        error!("Failed loading configuration: {}", clap_err);
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
    run_component_servers(servers, config).await?;

    // TODO(Tsabary): Add graceful shutdown.
    Ok(())
}
