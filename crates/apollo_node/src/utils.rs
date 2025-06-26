use std::process::exit;

use apollo_config::presentation::get_config_presentation;
use apollo_config::validators::config_validate;
use apollo_config::ConfigError;
use tracing::{error, info};

use crate::clients::{create_node_clients, SequencerNodeClients};
use crate::communication::create_node_channels;
use crate::components::create_node_components;
use crate::config::node_config::SequencerNodeConfig;
use crate::servers::{create_node_servers, SequencerNodeServers};

pub async fn create_node_modules(
    config: &SequencerNodeConfig,
) -> (SequencerNodeClients, SequencerNodeServers) {
    info!("Creating node modules.");

    let mut channels = create_node_channels(config);
    let clients = create_node_clients(config, &mut channels);
    let components = create_node_components(config, &clients).await;
    let servers = create_node_servers(config, &mut channels, components, &clients);

    (clients, servers)
}

pub fn load_and_validate_config(args: Vec<String>) -> Result<SequencerNodeConfig, ConfigError> {
    let config = SequencerNodeConfig::load_and_process(args);
    if let Err(ConfigError::CommandInput(clap_err)) = &config {
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

    info!("Config map:");
    info!(
        "{:#?}",
        get_config_presentation::<SequencerNodeConfig>(&config, false)
            .expect("Should be able to get representation.")
    );
    info!("Finished dumping configuration.");

    Ok(config)
}
