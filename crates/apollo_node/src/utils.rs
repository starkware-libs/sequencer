use apollo_config::presentation::get_config_presentation;
use apollo_config::ConfigError;
use tracing::info;

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
    let config_load_result = SequencerNodeConfig::load_and_process(args);
<<<<<<< HEAD
    let config =
        config_load_result.unwrap_or_else(|err| panic!("Failed loading configuration: {err}"));
||||||| 38f03e1d0
    let config =
        config_load_result.unwrap_or_else(|err| panic!("Failed loading configuration: {}", err));
=======
    let loaded_config =
        config_load_result.unwrap_or_else(|err| panic!("Failed loading configuration: {err}"));
>>>>>>> origin/main-v0.14.0
    info!("Finished loading configuration.");

<<<<<<< HEAD
    if let Err(error) = config_validate(&config) {
        panic!("Config validation failed: {error}");
||||||| 38f03e1d0
    if let Err(error) = config_validate(&config) {
        panic!("Config validation failed: {}", error);
=======
    if let Err(error) = loaded_config.validate_node_config() {
        panic!("Config validation failed: {error}");
>>>>>>> origin/main-v0.14.0
    }
    info!("Finished validating configuration.");

    info!("Config map:");
    info!(
        "{:#?}",
        get_config_presentation::<SequencerNodeConfig>(&loaded_config, false)
            .expect("Should be able to get representation.")
    );
    info!("Finished dumping configuration.");

    Ok(loaded_config)
}
