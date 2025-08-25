use std::collections::BTreeMap;
use std::path::PathBuf;
use std::result;

use apollo_central_sync::sources::central::CentralSourceConfig;
use apollo_central_sync::SyncConfig;
use apollo_config::dumping::{
    prepend_sub_config_name,
    ser_optional_sub_config,
    ser_param,
    SerializeConfig,
};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_network::NetworkConfig;
use apollo_p2p_sync::client::P2pSyncClientConfig;
use apollo_reverts::RevertConfig;
use apollo_rpc::RpcConfig;
use apollo_storage::db::DbConfig;
use apollo_storage::StorageConfig;
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

const STATE_SYNC_TCP_PORT: u16 = 12345;

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
    // TODO(noamsp): Remove this after fixing the replay procedure.
    // This is a temporary solution by disabling the replay procedure in production and enabling it
    // in integration tests.
    pub should_replay_processed_txs_metric: bool,
}

impl SerializeConfig for StateSyncConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = BTreeMap::from_iter([ser_param(
            "should_replay_processed_txs_metric",
            &self.should_replay_processed_txs_metric,
            "Whether to replay processed transactions.",
            ParamPrivacyInput::Public,
        )]);
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
            network_config: Some(NetworkConfig { port: STATE_SYNC_TCP_PORT, ..Default::default() }),
            revert_config: RevertConfig::default(),
            rpc_config: RpcConfig::default(),
            should_replay_processed_txs_metric: false,
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
