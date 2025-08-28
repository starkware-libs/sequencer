#[cfg(test)]
mod config_test;
#[cfg(feature = "rpc")]
pub mod pointers;

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::mem::discriminant;
use std::ops::IndexMut;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{env, fs, io};

use apollo_central_sync::sources::central::CentralSourceConfig;
use apollo_central_sync::SyncConfig;
use apollo_config::dumping::{
    prepend_sub_config_name,
    ser_optional_sub_config,
    ser_param,
    ser_pointer_target_param,
    SerializeConfig,
};
use apollo_config::loading::load_and_process_config;
use apollo_config::{ConfigError, ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_consensus::config::ConsensusConfig;
use apollo_consensus_orchestrator_config::ContextConfig;
use apollo_network::NetworkConfig;
use apollo_p2p_sync::client::{P2pSyncClient, P2pSyncClientConfig};
#[cfg(feature = "rpc")]
use apollo_rpc::RpcConfig;
use apollo_starknet_client::RetryConfig;
use apollo_storage::db::DbConfig;
use apollo_storage::StorageConfig;
use clap::{arg, value_parser, Arg, ArgMatches, Command};
use itertools::{chain, Itertools};
use lazy_static::lazy_static;
use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerConfig;
use papyrus_monitoring_gateway::MonitoringGatewayConfig;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use starknet_api::core::ChainId;
use validator::Validate;

use crate::version::VERSION_FULL;

// The path of the default configuration file, provided as part of the crate.
pub const DEFAULT_CONFIG_PATH: &str = "config/papyrus/default_config.json";

/// The configurations of the various components of the node.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct NodeConfig {
    #[cfg(feature = "rpc")]
    #[validate]
    pub rpc: RpcConfig,
    pub central: CentralSourceConfig,
    pub base_layer: EthereumBaseLayerConfig,
    pub monitoring_gateway: MonitoringGatewayConfig,
    #[validate]
    pub storage: StorageConfig,
    /// None if the syncing should be disabled.
    pub sync: Option<SyncConfig>,
    /// One of p2p_sync or sync must be None.
    /// If p2p sync is active, then network must be active too.
    // TODO(yair): Change NodeConfig to have an option of enum of SyncConfig or P2pSyncConfig.
    pub p2p_sync: Option<P2pSyncClientConfig>,
    pub consensus: Option<ConsensusConfig>,
    pub context: Option<ContextConfig>,
    // TODO(shahak): Make network non-optional once it's developed enough.
    pub network: Option<NetworkConfig>,
    pub collect_profiling_metrics: bool,
}

// Default configuration values.
impl Default for NodeConfig {
    fn default() -> Self {
        NodeConfig {
            central: CentralSourceConfig::default(),
            base_layer: EthereumBaseLayerConfig::default(),
            #[cfg(feature = "rpc")]
            rpc: RpcConfig::default(),
            monitoring_gateway: MonitoringGatewayConfig::default(),
            storage: StorageConfig::default(),
            sync: Some(SyncConfig { store_sierras_and_casms: true, ..Default::default() }),
            p2p_sync: None,
            consensus: None,
            context: None,
            network: None,
            collect_profiling_metrics: false,
        }
    }
}

impl SerializeConfig for NodeConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        #[allow(unused_mut)]
        let mut sub_configs = vec![
            prepend_sub_config_name(self.central.dump(), "central"),
            prepend_sub_config_name(self.base_layer.dump(), "base_layer"),
            prepend_sub_config_name(self.monitoring_gateway.dump(), "monitoring_gateway"),
            prepend_sub_config_name(self.storage.dump(), "storage"),
            ser_optional_sub_config(&self.sync, "sync"),
            ser_optional_sub_config(&self.p2p_sync, "p2p_sync"),
            ser_optional_sub_config(&self.consensus, "consensus"),
            ser_optional_sub_config(&self.context, "context"),
            ser_optional_sub_config(&self.network, "network"),
            BTreeMap::from_iter([ser_param(
                "collect_profiling_metrics",
                &self.collect_profiling_metrics,
                "If true, collect profiling metrics for the node.",
                ParamPrivacyInput::Public,
            )]),
        ];
        #[cfg(feature = "rpc")]
        sub_configs.push(prepend_sub_config_name(self.rpc.dump(), "rpc"));

        sub_configs.into_iter().flatten().collect()
    }
}

impl NodeConfig {
    /// Creates a config object. Selects the values from the default file and from resources with
    /// higher priority.
    pub fn load_and_process(args: Vec<String>) -> Result<Self, ConfigError> {
        let default_config_file = std::fs::File::open(Path::new(DEFAULT_CONFIG_PATH))?;
        load_and_process_config(default_config_file, node_command(), args, false)
    }
}

/// The command line interface of this node.
pub fn node_command() -> Command {
    Command::new("Papyrus")
        .version(VERSION_FULL)
        .about("Papyrus is a StarkNet full node written in Rust.")
}
