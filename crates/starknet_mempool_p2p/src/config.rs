use std::collections::BTreeMap;

use papyrus_config::dumping::{append_sub_config_name, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use papyrus_network::NetworkConfig;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct MempoolP2pConfig {
    #[validate]
    pub network_config: NetworkConfig,
    pub network_buffer_size: usize,
}

impl Default for MempoolP2pConfig {
    fn default() -> Self {
        Self { network_config: NetworkConfig::default(), network_buffer_size: 10000 }
    }
}

impl SerializeConfig for MempoolP2pConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        vec![
            BTreeMap::from_iter([ser_param(
                "network_buffer_size",
                &self.network_buffer_size,
                "Network buffer size.",
                ParamPrivacyInput::Public,
            )]),
            append_sub_config_name(self.network_config.dump(), "network_config"),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}
