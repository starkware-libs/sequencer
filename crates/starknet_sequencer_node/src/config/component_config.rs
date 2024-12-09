use std::collections::BTreeMap;

use papyrus_config::dumping::{append_sub_config_name, SerializeConfig};
use papyrus_config::{ParamPath, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::config::component_execution_config::{
    ActiveComponentExecutionConfig,
    ReactiveComponentExecutionConfig,
};

/// The components configuration.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct ComponentConfig {
    // Reactive component configs.
    #[validate]
    pub batcher: ReactiveComponentExecutionConfig,
    #[validate]
    pub consensus_manager: ReactiveComponentExecutionConfig,
    #[validate]
    pub gateway: ReactiveComponentExecutionConfig,
    #[validate]
    pub http_server: ReactiveComponentExecutionConfig,
    #[validate]
    pub mempool: ReactiveComponentExecutionConfig,
    #[validate]
    pub mempool_p2p: ReactiveComponentExecutionConfig,
    #[validate]
    pub state_sync: ReactiveComponentExecutionConfig,

    // Reactive component configs.
    pub monitoring_endpoint: ActiveComponentExecutionConfig,
}

impl Default for ComponentConfig {
    fn default() -> Self {
        Self {
            batcher: ReactiveComponentExecutionConfig::batcher_default_config(),
            consensus_manager: ReactiveComponentExecutionConfig::consensus_manager_default_config(),
            gateway: ReactiveComponentExecutionConfig::gateway_default_config(),
            http_server: ReactiveComponentExecutionConfig::http_server_default_config(),
            mempool: ReactiveComponentExecutionConfig::mempool_default_config(),
            mempool_p2p: ReactiveComponentExecutionConfig::mempool_p2p_default_config(),
            state_sync: ReactiveComponentExecutionConfig::state_sync_default_config(),
            // Reactive component configs.
            monitoring_endpoint: Default::default(),
        }
    }
}

impl SerializeConfig for ComponentConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let sub_configs = vec![
            append_sub_config_name(self.batcher.dump(), "batcher"),
            append_sub_config_name(self.consensus_manager.dump(), "consensus_manager"),
            append_sub_config_name(self.gateway.dump(), "gateway"),
            append_sub_config_name(self.http_server.dump(), "http_server"),
            append_sub_config_name(self.mempool.dump(), "mempool"),
            append_sub_config_name(self.mempool_p2p.dump(), "mempool_p2p"),
            append_sub_config_name(self.monitoring_endpoint.dump(), "monitoring_endpoint"),
            append_sub_config_name(self.state_sync.dump(), "state_sync"),
        ];

        sub_configs.into_iter().flatten().collect()
    }
}
