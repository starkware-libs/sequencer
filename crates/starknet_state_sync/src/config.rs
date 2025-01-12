use std::collections::BTreeMap;
use std::path::PathBuf;

use papyrus_config::dumping::{append_sub_config_name, SerializeConfig};
use papyrus_config::{ParamPath, SerializedParam};
use papyrus_network::NetworkConfig;
use papyrus_p2p_sync::client::P2pSyncClientConfig;
use papyrus_storage::db::DbConfig;
use papyrus_storage::StorageConfig;
use serde::{Deserialize, Serialize};
use validator::Validate;

const STATE_SYNC_TCP_PORT: u16 = 12345;

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct StateSyncConfig {
    #[validate]
    pub storage_config: StorageConfig,
    // TODO(shahak): add validate to P2pSyncClientConfig
    pub p2p_sync_client_config: P2pSyncClientConfig,
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

impl Default for StateSyncConfig {
    fn default() -> Self {
        Self {
            storage_config: StorageConfig {
                db_config: DbConfig {
                    path_prefix: PathBuf::from("./sequencer_data"),
                    ..Default::default()
                },
                ..Default::default()
            },
            p2p_sync_client_config: Default::default(),
            network_config: NetworkConfig { tcp_port: STATE_SYNC_TCP_PORT, ..Default::default() },
        }
    }
}
