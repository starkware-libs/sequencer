use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

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
    pub gateway: ReactiveComponentExecutionConfig,
    #[validate]
    pub mempool: ReactiveComponentExecutionConfig,
    #[validate]
    pub mempool_p2p: ReactiveComponentExecutionConfig,
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
    pub monitoring_endpoint: ActiveComponentExecutionConfig,
}

impl SerializeConfig for ComponentConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let sub_configs = vec![
            append_sub_config_name(self.batcher.dump(), "batcher"),
            append_sub_config_name(self.consensus_manager.dump(), "consensus_manager"),
            append_sub_config_name(self.gateway.dump(), "gateway"),
            append_sub_config_name(self.http_server.dump(), "http_server"),
            append_sub_config_name(self.mempool.dump(), "mempool"),
            append_sub_config_name(self.l1_provider.dump(), "l1_provider"),
            append_sub_config_name(self.mempool_p2p.dump(), "mempool_p2p"),
            append_sub_config_name(self.monitoring_endpoint.dump(), "monitoring_endpoint"),
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
            gateway: ReactiveComponentExecutionConfig::disabled(),
            mempool: ReactiveComponentExecutionConfig::disabled(),
            mempool_p2p: ReactiveComponentExecutionConfig::disabled(),
            state_sync: ReactiveComponentExecutionConfig::disabled(),
            l1_provider: ReactiveComponentExecutionConfig::disabled(),
            consensus_manager: ActiveComponentExecutionConfig::disabled(),
            http_server: ActiveComponentExecutionConfig::disabled(),
            monitoring_endpoint: ActiveComponentExecutionConfig::disabled(),
        }
    }

    pub fn get_default_for_testing() -> ComponentConfig {
        let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 8080);
        ComponentConfig {
            batcher: ReactiveComponentExecutionConfig::local_with_remote_disabled_for_testing(
                socket,
            ),
            gateway: ReactiveComponentExecutionConfig::local_with_remote_disabled_for_testing(
                socket,
            ),
            mempool: ReactiveComponentExecutionConfig::local_with_remote_disabled_for_testing(
                socket,
            ),
            mempool_p2p: ReactiveComponentExecutionConfig::local_with_remote_disabled_for_testing(
                socket,
            ),
            state_sync: ReactiveComponentExecutionConfig::local_with_remote_disabled_for_testing(
                socket,
            ),
            l1_provider: ReactiveComponentExecutionConfig::local_with_remote_disabled_for_testing(
                socket,
            ),
            consensus_manager: ActiveComponentExecutionConfig::enabled(),
            http_server: ActiveComponentExecutionConfig::enabled(),
            monitoring_endpoint: ActiveComponentExecutionConfig::enabled(),
        }
    }
}
