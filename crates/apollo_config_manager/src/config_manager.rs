use std::sync::Arc;

use apollo_batcher_config::config::BatcherDynamicConfig;
use apollo_class_manager_config::config::ClassManagerDynamicConfig;
use apollo_config_manager_config::config::ConfigManagerConfig;
use apollo_config_manager_types::communication::{ConfigManagerRequest, ConfigManagerResponse};
use apollo_config_manager_types::config_manager_types::ConfigManagerResult;
use apollo_consensus_config::config::ConsensusDynamicConfig;
use apollo_consensus_orchestrator_config::config::ContextDynamicConfig;
use apollo_gateway_config::config::GatewayDynamicConfig;
use apollo_http_server_config::config::HttpServerDynamicConfig;
use apollo_infra::component_definitions::{ComponentRequestHandler, ComponentStarter};
use apollo_mempool_config::config::MempoolDynamicConfig;
use apollo_node_config::node_config::NodeDynamicConfig;
use apollo_staking_config::config::StakingManagerDynamicConfig;
use apollo_state_sync_config::config::StateSyncDynamicConfig;
use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing::info;

#[cfg(test)]
#[path = "config_manager_tests.rs"]
pub mod config_manager_tests;

/// Expands to a match on `request`; each `($variant, $method)` pair becomes an arm that
/// calls `self.$method().await` and wraps the result in `ConfigManagerResponse::$variant`.
macro_rules! handle_config_request {
    ($self:expr, $request:expr, $( ($variant:ident, $method:ident) ),* $(,)?) => {
        match $request {
            $(
                ConfigManagerRequest::$variant => {
                    ConfigManagerResponse::$variant($self.$method().await)
                }
            ),*
            ConfigManagerRequest::SetNodeDynamicConfig(new_config) => {
                ConfigManagerResponse::SetNodeDynamicConfig(
                    $self.set_node_dynamic_config(*new_config).await,
                )
            }
        }
    };
}

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

    pub(crate) async fn get_class_manager_dynamic_config(
        &self,
    ) -> ConfigManagerResult<ClassManagerDynamicConfig> {
        let config = self.latest_node_dynamic_config.read().await;
        Ok(config.class_manager_dynamic_config.as_ref().unwrap().clone())
    }

    pub(crate) async fn get_context_dynamic_config(
        &self,
    ) -> ConfigManagerResult<ContextDynamicConfig> {
        let config = self.latest_node_dynamic_config.read().await;
        Ok(config.context_dynamic_config.as_ref().unwrap().clone())
    }

    pub(crate) async fn get_gateway_dynamic_config(
        &self,
    ) -> ConfigManagerResult<GatewayDynamicConfig> {
        let config = self.latest_node_dynamic_config.read().await;
        Ok(config.gateway_dynamic_config.as_ref().unwrap().clone())
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

    pub(crate) async fn get_state_sync_dynamic_config(
        &self,
    ) -> ConfigManagerResult<StateSyncDynamicConfig> {
        let config = self.latest_node_dynamic_config.read().await;
        Ok(config.state_sync_dynamic_config.as_ref().unwrap().clone())
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
        // Note: the `ConfigManagerRequest::SetNodeDynamicConfig` variant is handled inside the
        // macro.
        handle_config_request!(
            self,
            request,
            (GetBatcherDynamicConfig, get_batcher_dynamic_config),
            (GetClassManagerDynamicConfig, get_class_manager_dynamic_config),
            (GetConsensusDynamicConfig, get_consensus_dynamic_config),
            (GetContextDynamicConfig, get_context_dynamic_config),
            (GetGatewayDynamicConfig, get_gateway_dynamic_config),
            (GetHttpServerDynamicConfig, get_http_server_dynamic_config),
            (GetMempoolDynamicConfig, get_mempool_dynamic_config),
            (GetStakingManagerDynamicConfig, get_staking_manager_dynamic_config),
            (GetStateSyncDynamicConfig, get_state_sync_dynamic_config),
        )
    }
}

impl ComponentStarter for ConfigManager {}
