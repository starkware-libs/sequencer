use std::collections::BTreeMap;

use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerConfig;
use papyrus_config::dumping::{append_sub_config_name, SerializeConfig};
use papyrus_config::{ParamPath, SerializedParam};
use papyrus_storage::StorageConfig;
use papyrus_sync::sources::central::CentralSourceConfig;
use papyrus_sync::SyncConfig;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct StateSyncConfig {
    #[validate]
    pub storage_config: StorageConfig,
    // TODO(shahak): add validate to SyncConfig, CentralSourceConfig and EthereumBaseLayerConfig
    // and use them here.
    pub sync_config: SyncConfig,
    pub central_config: CentralSourceConfig,
    pub base_layer_config: EthereumBaseLayerConfig,
}

impl SerializeConfig for StateSyncConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        vec![
            append_sub_config_name(self.storage_config.dump(), "storage_config"),
            append_sub_config_name(self.sync_config.dump(), "sync_config"),
            append_sub_config_name(self.central_config.dump(), "central_config"),
            append_sub_config_name(self.base_layer_config.dump(), "base_layer_config"),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}
