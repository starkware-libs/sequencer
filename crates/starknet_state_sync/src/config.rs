use std::collections::BTreeMap;
use std::path::PathBuf;
use std::result;

use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerConfig;
use papyrus_config::dumping::{append_sub_config_name, ser_optional_sub_config, SerializeConfig};
use papyrus_config::{ParamPath, SerializedParam};
use papyrus_network::NetworkConfig;
use papyrus_p2p_sync::client::P2pSyncClientConfig;
use papyrus_storage::db::DbConfig;
use papyrus_storage::StorageConfig;
use papyrus_sync::sources::central::CentralSourceConfig;
use papyrus_sync::SyncConfig;
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

const STATE_SYNC_TCP_PORT: u16 = 12345;

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Validate)]
#[validate(schema(function = "validate_config"))]
pub struct StateSyncConfig {
    #[validate]
    pub storage_config: StorageConfig,
    // TODO(Eitan): Use enum configs instead of Option once supported.
    #[validate]
    pub p2p_sync_client_config: Option<P2pSyncClientConfig>,
    #[validate]
    pub central_sync_client_config: Option<CentralSyncClientConfig>,
    #[validate]
    pub network_config: NetworkConfig,
}

impl SerializeConfig for StateSyncConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        vec![
            append_sub_config_name(self.storage_config.dump(), "storage_config"),
            append_sub_config_name(self.network_config.dump(), "network_config"),
            ser_optional_sub_config(&self.p2p_sync_client_config, "p2p_sync_client_config"),
            ser_optional_sub_config(&self.central_sync_client_config, "central_sync_client_config"),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

fn validate_config(config: &StateSyncConfig) -> result::Result<(), ValidationError> {
    if config.central_sync_client_config.is_some() && config.p2p_sync_client_config.is_some()
        || config.central_sync_client_config.is_none() && config.p2p_sync_client_config.is_none()
    {
        return Err(ValidationError::new(
            "Exactly one of --sync.#is_none or --p2p_sync.#is_none must be turned on",
        ));
    }
    Ok(())
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
            p2p_sync_client_config: Some(P2pSyncClientConfig::default()),
            central_sync_client_config: None,
            network_config: NetworkConfig { tcp_port: STATE_SYNC_TCP_PORT, ..Default::default() },
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct CentralSyncClientConfig {
    pub sync_config: SyncConfig,
    pub central_source_config: CentralSourceConfig,
    pub base_layer_config: Option<EthereumBaseLayerConfig>,
}

impl SerializeConfig for CentralSyncClientConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        vec![
            append_sub_config_name(self.sync_config.dump(), "sync_config"),
            append_sub_config_name(self.central_source_config.dump(), "central_source_config"),
            ser_optional_sub_config(&self.base_layer_config, "base_layer_config"),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}
