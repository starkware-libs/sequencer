use std::collections::BTreeMap;

use papyrus_config::dumping::{append_sub_config_name, SerializeConfig};
use papyrus_config::{ParamPath, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::config::ComponentExecutionConfig;

/// The components configuration.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct ComponentConfig {
    #[validate]
    pub batcher: ComponentExecutionConfig,
    #[validate]
    pub consensus_manager: ComponentExecutionConfig,
    #[validate]
    pub gateway: ComponentExecutionConfig,
    #[validate]
    pub http_server: ComponentExecutionConfig,
    #[validate]
    pub mempool: ComponentExecutionConfig,
    #[validate]
    pub mempool_p2p: ComponentExecutionConfig,
    #[validate]
    pub monitoring_endpoint: ComponentExecutionConfig,
}

impl Default for ComponentConfig {
    fn default() -> Self {
        Self {
            batcher: ComponentExecutionConfig::batcher_default_config(),
            consensus_manager: ComponentExecutionConfig::consensus_manager_default_config(),
            gateway: ComponentExecutionConfig::gateway_default_config(),
            http_server: ComponentExecutionConfig::http_server_default_config(),
            mempool: ComponentExecutionConfig::mempool_default_config(),
            mempool_p2p: ComponentExecutionConfig::mempool_p2p_default_config(),
            monitoring_endpoint: ComponentExecutionConfig::monitoring_endpoint_default_config(),
        }
    }
}

impl SerializeConfig for ComponentConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        #[allow(unused_mut)]
        let mut sub_configs = vec![
            append_sub_config_name(self.batcher.dump(), "batcher"),
            append_sub_config_name(self.consensus_manager.dump(), "consensus_manager"),
            append_sub_config_name(self.gateway.dump(), "gateway"),
            append_sub_config_name(self.http_server.dump(), "http_server"),
            append_sub_config_name(self.mempool.dump(), "mempool"),
            append_sub_config_name(self.mempool_p2p.dump(), "mempool_p2p"),
            append_sub_config_name(self.monitoring_endpoint.dump(), "monitoring_endpoint"),
        ];

        sub_configs.into_iter().flatten().collect()
    }
}
