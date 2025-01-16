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
#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
pub struct ComponentConfig {
    // Reactive component configs.
    #[validate]
    pub batcher: ReactiveComponentExecutionConfig,
    #[validate]
    pub class_manager: ReactiveComponentExecutionConfig,
    #[validate]
    pub gateway: ReactiveComponentExecutionConfig,
    #[validate]
    pub mempool: ReactiveComponentExecutionConfig,
    #[validate]
    pub mempool_p2p: ReactiveComponentExecutionConfig,
    #[validate]
    pub sierra_compiler: ReactiveComponentExecutionConfig,
    #[validate]
    pub state_sync: ReactiveComponentExecutionConfig,
    #[validate]
    pub l1_provider: ReactiveComponentExecutionConfig,

    // Active component configs.
    #[validate]
    pub consensus_manager: ActiveComponentExecutionConfig,
    #[validate]
    pub http_server: ActiveComponentExecutionConfig,
    #[validate]
    pub l1_scraper: ActiveComponentExecutionConfig,
    #[validate]
    pub monitoring_endpoint: ActiveComponentExecutionConfig,
}

impl SerializeConfig for ComponentConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let sub_configs = vec![
            append_sub_config_name(self.batcher.dump(), "batcher"),
            append_sub_config_name(self.class_manager.dump(), "class_manager"),
            append_sub_config_name(self.consensus_manager.dump(), "consensus_manager"),
            append_sub_config_name(self.gateway.dump(), "gateway"),
            append_sub_config_name(self.http_server.dump(), "http_server"),
            append_sub_config_name(self.mempool.dump(), "mempool"),
            append_sub_config_name(self.l1_provider.dump(), "l1_provider"),
            append_sub_config_name(self.l1_scraper.dump(), "l1_scraper"),
            append_sub_config_name(self.mempool_p2p.dump(), "mempool_p2p"),
            append_sub_config_name(self.monitoring_endpoint.dump(), "monitoring_endpoint"),
            append_sub_config_name(self.sierra_compiler.dump(), "sierra_compiler"),
            append_sub_config_name(self.state_sync.dump(), "state_sync"),
        ];

        sub_configs.into_iter().flatten().collect()
    }
}

#[cfg(any(feature = "testing", test))]
impl ComponentConfig {
    pub fn disabled() -> ComponentConfig {
        ComponentConfig {
            batcher: ReactiveComponentExecutionConfig::disabled(),
            class_manager: ReactiveComponentExecutionConfig::disabled(),
            gateway: ReactiveComponentExecutionConfig::disabled(),
            mempool: ReactiveComponentExecutionConfig::disabled(),
            mempool_p2p: ReactiveComponentExecutionConfig::disabled(),
            sierra_compiler: ReactiveComponentExecutionConfig::disabled(),
            state_sync: ReactiveComponentExecutionConfig::disabled(),
            l1_provider: ReactiveComponentExecutionConfig::disabled(),
            l1_scraper: ActiveComponentExecutionConfig::disabled(),
            consensus_manager: ActiveComponentExecutionConfig::disabled(),
            http_server: ActiveComponentExecutionConfig::disabled(),
            monitoring_endpoint: ActiveComponentExecutionConfig::disabled(),
        }
    }
}
