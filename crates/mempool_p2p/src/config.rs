use std::collections::BTreeMap;

use papyrus_config::dumping::{append_sub_config_name, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use papyrus_network::NetworkConfig;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct MempoolP2pConfig {
    #[validate]
    pub network_config: NetworkConfig,
    // TODO: Enter this inside NetworkConfig
    pub executable_version: Option<String>,
    pub network_buffer_size: usize,
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
