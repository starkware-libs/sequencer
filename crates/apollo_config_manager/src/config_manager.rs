use std::sync::Arc;

use apollo_batcher_config::config::BatcherDynamicConfig;
use apollo_config_manager_config::config::ConfigManagerConfig;
use apollo_config_manager_types::communication::{ConfigManagerRequest, ConfigManagerResponse};
use apollo_config_manager_types::config_manager_types::ConfigManagerResult;
use apollo_consensus_config::config::ConsensusDynamicConfig;
use apollo_consensus_orchestrator_config::config::ContextDynamicConfig;
use apollo_http_server_config::config::HttpServerDynamicConfig;
use apollo_infra::component_definitions::{ComponentRequestHandler, ComponentStarter};
use apollo_mempool_config::config::MempoolDynamicConfig;
use apollo_node_config::node_config::NodeDynamicConfig;
use apollo_staking_config::config::StakingManagerDynamicConfig;
use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing::info;

#[cfg(test)]
#[path = "config_manager_tests.rs"]
pub mod config_manager_tests;

#[derive(Clone)]
pub struct ConfigManager {
    _config: ConfigManagerConfig,
    latest_node_dynamic_config: Arc<RwLock<NodeDynamicConfig>>,
}

impl ConfigManager {
    pub fn new(config: ConfigManagerConfig, node_dynamic_config: NodeDynamicConfig) -> Self {
        Self {
            _config: config,
            latest_node_dynamic_config: Arc::new(RwLock::new(node_dynamic_config)),
        }
    }

    pub(crate) async fn set_node_dynamic_config(
        &self,
        node_dynamic_config: NodeDynamicConfig,
    ) -> ConfigManagerResult<()> {
        info!("ConfigManager: updating node dynamic config");
        let mut config = self.latest_node_dynamic_config.write().await;
        *config = node_dynamic_config;
        Ok(())
    }

    pub(crate) async fn get_consensus_dynamic_config(
        &self,
    ) -> ConfigManagerResult<ConsensusDynamicConfig> {
        let config = self.latest_node_dynamic_config.read().await;
        Ok(config.consensus_dynamic_config.as_ref().unwrap().clone())
    }

    pub(crate) async fn get_context_dynamic_config(
        &self,
    ) -> ConfigManagerResult<ContextDynamicConfig> {
        let config = self.latest_node_dynamic_config.read().await;
        Ok(config.context_dynamic_config.as_ref().unwrap().clone())
    }

    pub(crate) async fn get_http_server_dynamic_config(
        &self,
    ) -> ConfigManagerResult<HttpServerDynamicConfig> {
        let config = self.latest_node_dynamic_config.read().await;
        Ok(config.http_server_dynamic_config.as_ref().unwrap().clone())
    }

    pub(crate) async fn get_mempool_dynamic_config(
        &self,
    ) -> ConfigManagerResult<MempoolDynamicConfig> {
        let config = self.latest_node_dynamic_config.read().await;
        Ok(config.mempool_dynamic_config.as_ref().unwrap().clone())
    }

    pub(crate) async fn get_batcher_dynamic_config(
        &self,
    ) -> ConfigManagerResult<BatcherDynamicConfig> {
        let config = self.latest_node_dynamic_config.read().await;
        Ok(config.batcher_dynamic_config.as_ref().unwrap().clone())
    }

    pub(crate) async fn get_staking_manager_dynamic_config(
        &self,
    ) -> ConfigManagerResult<StakingManagerDynamicConfig> {
        let config = self.latest_node_dynamic_config.read().await;
        Ok(config.staking_manager_dynamic_config.as_ref().unwrap().clone())
    }
}

#[async_trait]
impl ComponentRequestHandler<ConfigManagerRequest, ConfigManagerResponse> for ConfigManager {
    async fn handle_request(&mut self, request: ConfigManagerRequest) -> ConfigManagerResponse {
        match request {
            // TODO(Nadin/Tsabary): consider using a macro to generate the responses for each type
            // of request.
            ConfigManagerRequest::GetConsensusDynamicConfig => {
                ConfigManagerResponse::GetConsensusDynamicConfig(
                    self.get_consensus_dynamic_config().await,
                )
            }
            ConfigManagerRequest::GetMempoolDynamicConfig => {
                ConfigManagerResponse::GetMempoolDynamicConfig(
                    self.get_mempool_dynamic_config().await,
                )
            }
            ConfigManagerRequest::GetBatcherDynamicConfig => {
                ConfigManagerResponse::GetBatcherDynamicConfig(
                    self.get_batcher_dynamic_config().await,
                )
            }
            ConfigManagerRequest::GetStakingManagerDynamicConfig => {
                ConfigManagerResponse::GetStakingManagerDynamicConfig(
                    self.get_staking_manager_dynamic_config().await,
                )
            }
            ConfigManagerRequest::GetHttpServerDynamicConfig => {
                ConfigManagerResponse::GetHttpServerDynamicConfig(
                    self.get_http_server_dynamic_config().await,
                )
            }
            ConfigManagerRequest::GetContextDynamicConfig => {
                ConfigManagerResponse::GetContextDynamicConfig(
                    self.get_context_dynamic_config().await,
                )
            }
            ConfigManagerRequest::SetNodeDynamicConfig(new_config) => {
                ConfigManagerResponse::SetNodeDynamicConfig(
                    self.set_node_dynamic_config(*new_config).await,
                )
            }
        }
    }
}

impl ComponentStarter for ConfigManager {}
