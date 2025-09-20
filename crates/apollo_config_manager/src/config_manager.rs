use std::sync::Arc;

use apollo_config_manager_config::config::ConfigManagerConfig;
use apollo_config_manager_types::communication::{ConfigManagerRequest, ConfigManagerResponse};
use apollo_config_manager_types::config_manager_types::ConfigManagerResult;
use apollo_infra::component_definitions::{ComponentRequestHandler, ComponentStarter};
use apollo_infra::component_server::{ConcurrentLocalComponentServer, RemoteComponentServer};
use apollo_node_config::node_config::NodeDynamicConfig;
use async_trait::async_trait;
use serde_json::Value;
use tracing::{info, instrument};

/// Internal state management for the ConfigManager.
#[derive(Clone)]
pub struct ConfigManagerState {
    pub node_dynamic_config: Arc<NodeDynamicConfig>,
}

impl Default for ConfigManagerState {
    fn default() -> Self {
        Self { node_dynamic_config: Arc::new(NodeDynamicConfig::default()) }
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
        let config_json = serde_json::to_value(&*self.state.node_dynamic_config)
            .unwrap_or_else(|_| serde_json::json!({}));
        Arc::new(config_json)
    }

    pub fn get_node_dynamic_config(&self) -> Arc<NodeDynamicConfig> {
        self.state.node_dynamic_config.clone()
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
            ConfigManagerRequest::GetNodeDynamicConfig => {
                info!("ConfigManager: handling GetNodeDynamicConfig request");
                let node_config = self.get_node_dynamic_config();
                let result = Ok((*node_config).clone());
                ConfigManagerResponse::GetNodeDynamicConfig(result)
            }
        }
    }
}

impl ComponentStarter for ConfigManager {}
