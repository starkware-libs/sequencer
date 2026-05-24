use std::path::Path;

use apollo_config::ConfigError;
use apollo_infra_utils::dumping::serialize_to_file;
use serde_json::Value;
use tracing::{error, info};

use crate::node_config::SequencerNodeConfig;

/// Dotted-path list of all private (secret) fields in SequencerNodeConfig.
/// These paths are used to strip secrets from the config presentation.
pub const PRIVATE_FIELD_PATHS: &[&str] = &[
    "base_layer_config.ordered_l1_endpoint_urls",
    "consensus_manager_config.network_config.secret_key",
    "l1_gas_price_provider_config.eth_to_strk_oracle_config.url_header_list",
    "mempool_p2p_config.network_config.secret_key",
    "state_sync_config.static_config.central_sync_client_config.central_source_config.http_headers",
    "state_sync_config.static_config.network_config.secret_key",
];

// TODO(Nadin/Tsabary): `DeploymentBaseAppConfig` is only used in tests, and should be marked as
// such.
#[derive(Debug, Clone, Default)]
pub struct DeploymentBaseAppConfig {
    pub config: SequencerNodeConfig,
}

impl DeploymentBaseAppConfig {
    pub fn new(config: SequencerNodeConfig) -> Self {
        Self { config }
    }

    pub fn get_config(&self) -> &SequencerNodeConfig {
        &self.config
    }

    pub fn modify_config<F>(&mut self, modify_config_fn: F)
    where
        F: Fn(&mut SequencerNodeConfig),
    {
        modify_config_fn(&mut self.config);
    }

    pub fn as_value(&self) -> Value {
        serde_json::to_value(&self.config).expect("SequencerNodeConfig should serialize to JSON")
    }

    // TODO(Tsabary): unify path types throughout.
    pub fn dump_config_file(&self, config_path: &Path) {
        let value = self.as_value();
        serialize_to_file(
            &value,
            config_path.to_str().expect("Should be able to convert path to string"),
        );
    }
}

pub fn load_and_validate_config(
    args: Vec<String>,
    log_enabled: bool,
) -> Result<SequencerNodeConfig, ConfigError> {
    let config_load_result = SequencerNodeConfig::load_and_process(args);
    if let Err(error) = config_load_result {
        error!("Failed loading configuration: {error}");
        return Err(error);
    }
    let loaded_config = config_load_result.unwrap();

    if log_enabled {
        info!("Finished loading configuration.");
    }

    let config_validation_result = loaded_config.validate_node_config();
    if let Err(error) = config_validation_result {
        error!("Config validation failed: {error}");
        return Err(error);
    }

    if log_enabled {
        info!("Finished validating configuration.");
        info!("Config map:");
        info!(
            "{:#?}",
            apollo_config::presentation::get_config_presentation(
                &loaded_config,
                PRIVATE_FIELD_PATHS,
                false,
            )
            .expect("Should be able to get representation.")
        );
        info!("Finished dumping configuration.");
    }

    Ok(loaded_config)
}
