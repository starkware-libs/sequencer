use std::collections::BTreeMap;

use papyrus_config::dumping::{append_sub_config_name, SerializeConfig};
use papyrus_config::{ParamPath, SerializedParam};
use papyrus_network::NetworkConfig;
use papyrus_p2p_sync::client::P2PSyncClientConfig;
use papyrus_storage::StorageConfig;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct StateSyncConfig {
    #[validate]
    pub storage_config: StorageConfig,
    // TODO(shahak): add validate to P2PSyncClientConfig
    pub p2p_sync_client_config: P2PSyncClientConfig,
    #[validate]
    pub network_config: NetworkConfig,
}

impl SerializeConfig for StateSyncConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        vec![
            append_sub_config_name(self.storage_config.dump(), "storage_config"),
            append_sub_config_name(self.p2p_sync_client_config.dump(), "p2p_sync_client_config"),
            append_sub_config_name(self.network_config.dump(), "network_config"),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}
