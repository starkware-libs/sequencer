use std::sync::Arc;

use apollo_config_manager_types::communication::{ConfigManagerRequest, ConfigManagerResponse};
use apollo_config_manager_types::config_manager_types::ConfigManagerResult;
use apollo_infra::component_definitions::{ComponentRequestHandler, ComponentStarter};
use apollo_infra::component_server::{ConcurrentLocalComponentServer, RemoteComponentServer};
use async_trait::async_trait;
use serde_json::Value;
use tracing::{info, instrument};

use crate::config::ConfigManagerConfig;

#[allow(dead_code)]
pub struct ConfigManager {
    config: ConfigManagerConfig,
    // TODO(Nadin): Add actual config state/storage
    current_config: Arc<Value>,
}

impl ConfigManager {
    pub fn new(config: ConfigManagerConfig) -> Self {
        // TODO(Nadin): Initialize with actual config data
        let current_config = Arc::new(serde_json::json!({}));
        info!("ConfigManager initialized");

        Self { config, current_config }
    }

    pub async fn update_config(&mut self) -> ConfigManagerResult<()> {
        // TODO(Nadin): Implement actual config update logic
        info!("ConfigManager: updating configuration");

        Ok(())
    }

    pub fn get_current_config(&self) -> Arc<Value> {
        self.current_config.clone()
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
        }
    }
}

impl ComponentStarter for ConfigManager {}
