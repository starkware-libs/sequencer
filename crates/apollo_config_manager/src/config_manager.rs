use apollo_batcher_config::config::BatcherDynamicConfig;
use apollo_class_manager_config::config::ClassManagerDynamicConfig;
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
use tracing::info;

#[cfg(test)]
#[path = "config_manager_tests.rs"]
pub mod config_manager_tests;

#[derive(Clone)]
pub struct ConfigManager {
    _config: ConfigManagerConfig,
    latest_node_dynamic_config: NodeDynamicConfig,
}

impl ConfigManager {
    pub fn new(config: ConfigManagerConfig, node_dynamic_config: NodeDynamicConfig) -> Self {
        Self { _config: config, latest_node_dynamic_config: node_dynamic_config }
    }

    pub(crate) fn set_node_dynamic_config(
        &mut self,
        node_dynamic_config: NodeDynamicConfig,
    ) -> ConfigManagerResult<()> {
        info!("ConfigManager: updating node dynamic config");
        self.latest_node_dynamic_config = node_dynamic_config;
        Ok(())
    }

    pub(crate) fn get_consensus_dynamic_config(
        &self,
    ) -> ConfigManagerResult<ConsensusDynamicConfig> {
        Ok(self.latest_node_dynamic_config.consensus_dynamic_config.as_ref().unwrap().clone())
    }

    pub(crate) fn get_class_manager_dynamic_config(
        &self,
    ) -> ConfigManagerResult<ClassManagerDynamicConfig> {
        Ok(self.latest_node_dynamic_config.class_manager_dynamic_config.as_ref().unwrap().clone())
    }

    pub(crate) fn get_context_dynamic_config(&self) -> ConfigManagerResult<ContextDynamicConfig> {
        Ok(self.latest_node_dynamic_config.context_dynamic_config.as_ref().unwrap().clone())
    }

    pub(crate) fn get_http_server_dynamic_config(
        &self,
    ) -> ConfigManagerResult<HttpServerDynamicConfig> {
        Ok(self.latest_node_dynamic_config.http_server_dynamic_config.as_ref().unwrap().clone())
    }

    pub(crate) fn get_mempool_dynamic_config(&self) -> ConfigManagerResult<MempoolDynamicConfig> {
        Ok(self.latest_node_dynamic_config.mempool_dynamic_config.as_ref().unwrap().clone())
    }

    pub(crate) fn get_batcher_dynamic_config(&self) -> ConfigManagerResult<BatcherDynamicConfig> {
        Ok(self.latest_node_dynamic_config.batcher_dynamic_config.as_ref().unwrap().clone())
    }

    pub(crate) fn get_staking_manager_dynamic_config(
        &self,
    ) -> ConfigManagerResult<StakingManagerDynamicConfig> {
        Ok(self.latest_node_dynamic_config.staking_manager_dynamic_config.as_ref().unwrap().clone())
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
                    self.get_consensus_dynamic_config(),
                )
            }
            ConfigManagerRequest::GetClassManagerDynamicConfig => {
                ConfigManagerResponse::GetClassManagerDynamicConfig(
                    self.get_class_manager_dynamic_config(),
                )
            }
            ConfigManagerRequest::GetContextDynamicConfig => {
                ConfigManagerResponse::GetContextDynamicConfig(self.get_context_dynamic_config())
            }
            ConfigManagerRequest::GetHttpServerDynamicConfig => {
                ConfigManagerResponse::GetHttpServerDynamicConfig(
                    self.get_http_server_dynamic_config(),
                )
            }
            ConfigManagerRequest::GetMempoolDynamicConfig => {
                ConfigManagerResponse::GetMempoolDynamicConfig(self.get_mempool_dynamic_config())
            }
            ConfigManagerRequest::GetBatcherDynamicConfig => {
                ConfigManagerResponse::GetBatcherDynamicConfig(self.get_batcher_dynamic_config())
            }
            ConfigManagerRequest::GetStakingManagerDynamicConfig => {
                ConfigManagerResponse::GetStakingManagerDynamicConfig(
                    self.get_staking_manager_dynamic_config(),
                )
            }
            ConfigManagerRequest::SetNodeDynamicConfig(new_config) => {
                ConfigManagerResponse::SetNodeDynamicConfig(
                    self.set_node_dynamic_config(*new_config),
                )
            }
        }
    }
}

impl ComponentStarter for ConfigManager {}
