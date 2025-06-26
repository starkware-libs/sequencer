use std::collections::BTreeMap;

use apollo_config::dumping::{prepend_sub_config_name, SerializeConfig};
use apollo_config::{ParamPath, SerializedParam};
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
    pub signature_manager: ReactiveComponentExecutionConfig,
    #[validate]
    pub state_sync: ReactiveComponentExecutionConfig,
    #[validate]
    pub l1_endpoint_monitor: ReactiveComponentExecutionConfig,
    #[validate]
    pub l1_provider: ReactiveComponentExecutionConfig,
    #[validate]
    pub l1_gas_price_provider: ReactiveComponentExecutionConfig,

    // Active component configs.
    #[validate]
    pub consensus_manager: ActiveComponentExecutionConfig,
    #[validate]
    pub http_server: ActiveComponentExecutionConfig,
    #[validate]
    pub l1_scraper: ActiveComponentExecutionConfig,
    #[validate]
    pub l1_gas_price_scraper: ActiveComponentExecutionConfig,
    #[validate]
    pub monitoring_endpoint: ActiveComponentExecutionConfig,
}

impl SerializeConfig for ComponentConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let sub_configs = vec![
            prepend_sub_config_name(self.batcher.dump(), "batcher"),
            prepend_sub_config_name(self.class_manager.dump(), "class_manager"),
            prepend_sub_config_name(self.consensus_manager.dump(), "consensus_manager"),
            prepend_sub_config_name(self.gateway.dump(), "gateway"),
            prepend_sub_config_name(self.http_server.dump(), "http_server"),
            prepend_sub_config_name(self.mempool.dump(), "mempool"),
            prepend_sub_config_name(self.l1_endpoint_monitor.dump(), "l1_endpoint_monitor"),
            prepend_sub_config_name(self.l1_provider.dump(), "l1_provider"),
            prepend_sub_config_name(self.l1_gas_price_provider.dump(), "l1_gas_price_provider"),
            prepend_sub_config_name(self.l1_scraper.dump(), "l1_scraper"),
            prepend_sub_config_name(self.l1_gas_price_scraper.dump(), "l1_gas_price_scraper"),
            prepend_sub_config_name(self.mempool_p2p.dump(), "mempool_p2p"),
            prepend_sub_config_name(self.monitoring_endpoint.dump(), "monitoring_endpoint"),
            prepend_sub_config_name(self.sierra_compiler.dump(), "sierra_compiler"),
            prepend_sub_config_name(self.signature_manager.dump(), "signature_manager"),
            prepend_sub_config_name(self.state_sync.dump(), "state_sync"),
        ];

        sub_configs.into_iter().flatten().collect()
    }
}

impl ComponentConfig {
    pub fn disabled() -> ComponentConfig {
        ComponentConfig {
            batcher: ReactiveComponentExecutionConfig::disabled(),
            class_manager: ReactiveComponentExecutionConfig::disabled(),
            gateway: ReactiveComponentExecutionConfig::disabled(),
            mempool: ReactiveComponentExecutionConfig::disabled(),
            mempool_p2p: ReactiveComponentExecutionConfig::disabled(),
            sierra_compiler: ReactiveComponentExecutionConfig::disabled(),
            signature_manager: ReactiveComponentExecutionConfig::disabled(),
            state_sync: ReactiveComponentExecutionConfig::disabled(),
            l1_endpoint_monitor: ReactiveComponentExecutionConfig::disabled(),
            l1_provider: ReactiveComponentExecutionConfig::disabled(),
            l1_gas_price_provider: ReactiveComponentExecutionConfig::disabled(),
            l1_scraper: ActiveComponentExecutionConfig::disabled(),
            l1_gas_price_scraper: ActiveComponentExecutionConfig::disabled(),
            consensus_manager: ActiveComponentExecutionConfig::disabled(),
            http_server: ActiveComponentExecutionConfig::disabled(),
            monitoring_endpoint: ActiveComponentExecutionConfig::disabled(),
        }
    }

    #[cfg(any(feature = "testing", test))]
    pub fn set_urls_to_localhost(&mut self) {
        self.batcher.set_url_to_localhost();
        self.class_manager.set_url_to_localhost();
        self.gateway.set_url_to_localhost();
        self.mempool.set_url_to_localhost();
        self.mempool_p2p.set_url_to_localhost();
        self.sierra_compiler.set_url_to_localhost();
        self.signature_manager.set_url_to_localhost();
        self.state_sync.set_url_to_localhost();
        self.l1_endpoint_monitor.set_url_to_localhost();
        self.l1_provider.set_url_to_localhost();
        self.l1_gas_price_provider.set_url_to_localhost();
    }
}

#[cfg(any(feature = "testing", test))]
pub fn set_urls_to_localhost(component_configs: &mut [ComponentConfig]) {
    for component_config in component_configs.iter_mut() {
        component_config.set_urls_to_localhost();
    }
}
