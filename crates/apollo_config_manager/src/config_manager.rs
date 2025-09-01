use std::sync::Arc;

use apollo_config_manager_types::communication::{ConfigManagerRequest, ConfigManagerResponse};
use apollo_config_manager_types::config_manager_types::ConfigManagerResult;
use apollo_consensus_config::ConsensusDynamicConfig;
use apollo_infra::component_definitions::{ComponentRequestHandler, ComponentStarter};
use apollo_infra::component_server::{ConcurrentLocalComponentServer, RemoteComponentServer};
use async_trait::async_trait;
use serde_json::Value;
use tracing::{info, instrument};

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
        // TODO(Nadin): Implement actual config update logic
        info!("ConfigManager: updating configuration");

        Ok(())
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
