use std::fs;
use std::sync::Arc;

use apollo_config_manager_types::communication::{ConfigManagerRequest, ConfigManagerResponse};
use apollo_config_manager_types::config_manager_types::ConfigManagerResult;
use apollo_config_manager_types::errors::ConfigManagerError;
use apollo_consensus_config::config::ConsensusDynamicConfig;
use apollo_consensus_config::ValidatorId;
use apollo_infra::component_definitions::{ComponentRequestHandler, ComponentStarter};
use apollo_infra::component_server::{ConcurrentLocalComponentServer, RemoteComponentServer};
use async_trait::async_trait;
use serde_json::Value;
use tracing::{error, info, instrument, warn};

use crate::config::ConfigManagerConfig;

/// Internal state management for the ConfigManager.
#[derive(Clone)]
pub struct ConfigManagerState {
    pub consensus_dynamic_config: Arc<ConsensusDynamicConfig>,
}

impl Default for ConfigManagerState {
    fn default() -> Self {
        Self { consensus_dynamic_config: Arc::new(ConsensusDynamicConfig::default()) }
    }
}

// TODO(Nadin): remove dead_code once we have actual config manager logic
#[allow(dead_code)]
#[derive(Clone)]
pub struct ConfigManager {
    config: ConfigManagerConfig,
    state: ConfigManagerState,
}

impl ConfigManager {
    pub fn new(config: ConfigManagerConfig) -> Self {
        let state = ConfigManagerState::default();
        info!("ConfigManager initialized with default configuration");
        Self { config, state }
    }

    pub async fn update_config(&mut self) -> ConfigManagerResult<()> {
        info!(
            "ConfigManager: updating configuration from file: {:?}",
            self.config.config_file_path
        );

        // Read and parse the config file
        let config_data = match self.read_config_file().await {
            Ok(data) => data,
            Err(e) => {
                error!("Failed to read config file: {:?}", e);
                return Err(e);
            }
        };

        // Extract consensus dynamic config from the config JSON
        let consensus_dynamic_config = match self.extract_consensus_dynamic_config(&config_data) {
            Ok(config) => config,
            Err(e) => {
                error!("Failed to extract consensus dynamic config: {:?}", e);
                return Err(e);
            }
        };

        // Update the internal state
        self.state.consensus_dynamic_config = Arc::new(consensus_dynamic_config);
        info!("ConfigManager: successfully updated consensus dynamic config");

        Ok(())
    }

    /// Reads and parses the configuration file.
    async fn read_config_file(&self) -> ConfigManagerResult<Value> {
        let config_content = fs::read_to_string(&self.config.config_file_path).map_err(|e| {
            ConfigManagerError::ConfigNotFound(format!("Failed to read config file: {}", e))
        })?;

        let config_value: Value = serde_json::from_str(&config_content).map_err(|e| {
            ConfigManagerError::ConfigParsingError(format!("Failed to parse JSON config: {}", e))
        })?;

        // Extract the config from ConfigMap format (expects "config" field with JSON string)
        let nested_config = config_value.get("config").ok_or_else(|| {
            ConfigManagerError::ConfigParsingError(
                "Expected config to be in ConfigMap format with 'config' field".to_string(),
            )
        })?;

        let config_data = if let Some(config_str) = nested_config.as_str() {
            // Parse the JSON string within the ConfigMap
            serde_json::from_str::<Value>(config_str).map_err(|e| {
                ConfigManagerError::ConfigParsingError(format!(
                    "Failed to parse nested JSON config: {}",
                    e
                ))
            })?
        } else {
            // If it's already parsed JSON (not a string), use it directly
            nested_config.clone()
        };

        Ok(config_data)
    }

    /// Extracts consensus dynamic configuration from the config JSON.
    pub(crate) fn extract_consensus_dynamic_config(
        &self,
        config_data: &Value,
    ) -> ConfigManagerResult<ConsensusDynamicConfig> {
        // TODO(Nadin): improve this logic
        #[allow(clippy::match_single_binding)]
        match ConsensusDynamicConfig::default() {
            ConsensusDynamicConfig { validator_id: default_validator_id } => {
                // Extract validator_id from config data
                const VALIDATOR_ID_KEY: &str =
                    "consensus_manager_config.consensus_manager_config.dynamic_config.validator_id";

                let validator_id = if let Some(value) = config_data.get(VALIDATOR_ID_KEY) {
                    info!("Found validator_id config key: {}", VALIDATOR_ID_KEY);

                    match serde_json::from_value::<ValidatorId>(value.clone()) {
                        Ok(parsed_id) => {
                            info!("Successfully deserialized validator_id: {:?}", value);
                            parsed_id
                        }
                        Err(e) => {
                            return Err(ConfigManagerError::ConfigParsingError(format!(
                                "Failed to deserialize validator_id {:?}: {}",
                                value, e
                            )));
                        }
                    }
                } else {
                    warn!("No validator_id found at key '{}', using default", VALIDATOR_ID_KEY);
                    default_validator_id
                };

                Ok(ConsensusDynamicConfig { validator_id })
            }
        }
    }

    pub fn get_current_config(&self) -> Arc<Value> {
        let config_json = serde_json::to_value(&*self.state.consensus_dynamic_config)
            .unwrap_or_else(|_| serde_json::json!({}));
        Arc::new(config_json)
    }

    pub fn get_consensus_dynamic_config(&self) -> Arc<ConsensusDynamicConfig> {
        self.state.consensus_dynamic_config.clone()
    }
}

pub type LocalConfigManagerServer =
    ConcurrentLocalComponentServer<ConfigManager, ConfigManagerRequest, ConfigManagerResponse>;
pub type RemoteConfigManagerServer =
    RemoteComponentServer<ConfigManagerRequest, ConfigManagerResponse>;

#[async_trait]
impl ComponentRequestHandler<ConfigManagerRequest, ConfigManagerResponse> for ConfigManager {
    #[instrument(skip(self), ret)]
    async fn handle_request(&mut self, request: ConfigManagerRequest) -> ConfigManagerResponse {
        match request {
            ConfigManagerRequest::ReadConfig => {
                info!("ConfigManager: handling ReadConfig request");
                let config_data = self.get_current_config();
                let result = Ok((*config_data).clone());
                ConfigManagerResponse::ReadConfig(result)
            }
            ConfigManagerRequest::GetConsensusDynamicConfig => {
                info!("ConfigManager: handling GetConsensusDynamicConfig request");
                let consensus_config = self.get_consensus_dynamic_config();
                let result = Ok((*consensus_config).clone());
                ConfigManagerResponse::GetConsensusDynamicConfig(result)
            }
        }
    }
}

impl ComponentStarter for ConfigManager {}
