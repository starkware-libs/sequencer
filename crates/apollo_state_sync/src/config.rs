use std::collections::BTreeMap;
use std::path::PathBuf;
use std::result;

use apollo_central_sync::sources::central::CentralSourceConfig;
use apollo_central_sync::SyncConfig;
use apollo_config::dumping::{prepend_sub_config_name, ser_optional_sub_config, SerializeConfig};
use apollo_config::{ParamPath, SerializedParam};
use apollo_network::NetworkConfig;
use apollo_p2p_sync::client::P2pSyncClientConfig;
use apollo_reverts::RevertConfig;
use apollo_rpc::RpcConfig;
use apollo_storage::db::DbConfig;
use apollo_storage::StorageConfig;
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

const STATE_SYNC_UDP_PORT: u16 = 12345;

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Validate)]
#[validate(schema(function = "validate_config"))]
pub struct StateSyncConfig {
    #[validate]
    pub storage_config: StorageConfig,
    // TODO(Eitan): Add support for enum configs and use here
    #[validate]
    pub p2p_sync_client_config: Option<P2pSyncClientConfig>,
    #[validate]
    pub central_sync_client_config: Option<CentralSyncClientConfig>,
    #[validate]
    pub network_config: Option<NetworkConfig>,
    #[validate]
    pub revert_config: RevertConfig,
    #[validate]
    pub rpc_config: RpcConfig,
}

impl SerializeConfig for StateSyncConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = BTreeMap::new();

        config.extend(prepend_sub_config_name(self.storage_config.dump(), "storage_config"));
        config.extend(ser_optional_sub_config(&self.network_config, "network_config"));
        config.extend(prepend_sub_config_name(self.revert_config.dump(), "revert_config"));
        config.extend(prepend_sub_config_name(self.rpc_config.dump(), "rpc_config"));
        config.extend(ser_optional_sub_config(
            &self.p2p_sync_client_config,
            "p2p_sync_client_config",
        ));
        config.extend(ser_optional_sub_config(
            &self.central_sync_client_config,
            "central_sync_client_config",
        ));
        config
    }
}

fn validate_config(config: &StateSyncConfig) -> result::Result<(), ValidationError> {
    if config.central_sync_client_config.is_some() && config.p2p_sync_client_config.is_some()
        || config.central_sync_client_config.is_none() && config.p2p_sync_client_config.is_none()
    {
        return Err(ValidationError::new(
            "Exactly one of --central_sync_client_config.#is_none or \
             --p2p_sync_client_config.#is_none must be turned on",
        ));
    }
    Ok(())
}

impl Default for StateSyncConfig {
    fn default() -> Self {
        Self {
            storage_config: StorageConfig {
                db_config: DbConfig {
                    path_prefix: PathBuf::from("/data/state_sync"),
                    ..Default::default()
                },
                ..Default::default()
            },
            p2p_sync_client_config: Some(P2pSyncClientConfig::default()),
            central_sync_client_config: None,
            network_config: Some(NetworkConfig { port: STATE_SYNC_UDP_PORT, ..Default::default() }),
            revert_config: RevertConfig::default(),
            rpc_config: RpcConfig::default(),
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct CentralSyncClientConfig {
    pub sync_config: SyncConfig,
    pub central_source_config: CentralSourceConfig,
}

impl SerializeConfig for CentralSyncClientConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        vec![
            prepend_sub_config_name(self.sync_config.dump(), "sync_config"),
            prepend_sub_config_name(self.central_source_config.dump(), "central_source_config"),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}
