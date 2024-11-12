use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::path::Path;
use std::sync::LazyLock;
use std::vec::Vec;

use clap::Command;
use papyrus_config::dumping::{
    append_sub_config_name,
    ser_pointer_target_required_param,
    set_pointing_param_paths,
    ConfigPointers,
    Pointers,
    SerializeConfig,
};
use papyrus_config::loading::load_and_process_config;
use papyrus_config::{ConfigError, ParamPath, SerializationType, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_batcher::config::BatcherConfig;
use starknet_consensus_manager::config::ConsensusManagerConfig;
use starknet_gateway::config::{GatewayConfig, RpcStateReaderConfig};
use starknet_http_server::config::HttpServerConfig;
use starknet_mempool_p2p::config::MempoolP2pConfig;
use starknet_monitoring_endpoint::config::MonitoringEndpointConfig;
use starknet_sierra_compile::config::SierraToCasmCompilationConfig;
use validator::Validate;

use crate::config::ComponentConfig;
use crate::utils::get_absolute_path;
use crate::version::VERSION_FULL;

// The path of the default configuration file, provided as part of the crate.
pub const DEFAULT_CONFIG_PATH: &str = "config/mempool/default_config.json";

// Configuration parameters that share the same value across multiple components.

// Required target parameters.
pub static REQUIRED_PARAM_CONFIG_POINTERS: LazyLock<ConfigPointers> = LazyLock::new(|| {
    vec![
        (
            ser_pointer_target_required_param(
                "chain_id",
                SerializationType::String,
                "The chain to follow.",
            ),
            set_pointing_param_paths(&[
                "batcher_config.block_builder_config.chain_info.chain_id",
                "batcher_config.storage.db_config.chain_id",
                "consensus_manager_config.consensus_config.network_config.chain_id",
                "gateway_config.chain_info.chain_id",
                "mempool_p2p_config.network_config.chain_id",
            ]),
        ),
        (
            ser_pointer_target_required_param(
                "eth_fee_token_address",
                SerializationType::String,
                "Address of the ETH fee token.",
            ),
            set_pointing_param_paths(&[
                "batcher_config.block_builder_config.chain_info.fee_token_addresses.\
                 eth_fee_token_address",
                "gateway_config.chain_info.fee_token_addresses.eth_fee_token_address",
            ]),
        ),
        (
            ser_pointer_target_required_param(
                "strk_fee_token_address",
                SerializationType::String,
                "Address of the STRK fee token.",
            ),
            set_pointing_param_paths(&[
                "batcher_config.block_builder_config.chain_info.fee_token_addresses.\
                 strk_fee_token_address",
                "gateway_config.chain_info.fee_token_addresses.strk_fee_token_address",
            ]),
        ),
    ]
});

// Optional target parameters, i.e., target parameters with default values.
pub static DEFAULT_PARAM_CONFIG_POINTERS: LazyLock<ConfigPointers> = LazyLock::new(Vec::new);

// All target parameters.
pub static CONFIG_POINTERS: LazyLock<ConfigPointers> = LazyLock::new(|| {
    let mut combined = REQUIRED_PARAM_CONFIG_POINTERS.clone();
    combined.extend(DEFAULT_PARAM_CONFIG_POINTERS.clone());
    combined
});

// Parameters that should 1) not be pointers, and 2) have a name matching a pointer target param.
pub static CONFIG_NON_POINTERS_WHITELIST: LazyLock<Pointers> =
    LazyLock::new(HashSet::<ParamPath>::new);

// TODO(yair): Make the GW and batcher execution config point to the same values.
/// The configurations of the various components of the node.
#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct SequencerNodeConfig {
    #[validate]
    pub components: ComponentConfig,
    #[validate]
    pub batcher_config: BatcherConfig,
    #[validate]
    pub consensus_manager_config: ConsensusManagerConfig,
    #[validate]
    pub gateway_config: GatewayConfig,
    #[validate]
    pub http_server_config: HttpServerConfig,
    #[validate]
    pub rpc_state_reader_config: RpcStateReaderConfig,
    #[validate]
    pub compiler_config: SierraToCasmCompilationConfig,
    #[validate]
    pub mempool_p2p_config: MempoolP2pConfig,
    #[validate]
    pub monitoring_endpoint_config: MonitoringEndpointConfig,
}

impl SerializeConfig for SequencerNodeConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let sub_configs = vec![
            append_sub_config_name(self.components.dump(), "components"),
            append_sub_config_name(self.batcher_config.dump(), "batcher_config"),
            append_sub_config_name(
                self.consensus_manager_config.dump(),
                "consensus_manager_config",
            ),
            append_sub_config_name(self.gateway_config.dump(), "gateway_config"),
            append_sub_config_name(self.http_server_config.dump(), "http_server_config"),
            append_sub_config_name(self.rpc_state_reader_config.dump(), "rpc_state_reader_config"),
            append_sub_config_name(self.compiler_config.dump(), "compiler_config"),
            append_sub_config_name(self.mempool_p2p_config.dump(), "mempool_p2p_config"),
            append_sub_config_name(
                self.monitoring_endpoint_config.dump(),
                "monitoring_endpoint_config",
            ),
        ];

        sub_configs.into_iter().flatten().collect()
    }
}

impl SequencerNodeConfig {
    /// Creates a config object. Selects the values from the default file and from resources with
    /// higher priority.
    fn load_and_process_config_file(
        args: Vec<String>,
        config_file_name: Option<&str>,
    ) -> Result<Self, ConfigError> {
        let config_file_name = match config_file_name {
            Some(file_name) => Path::new(file_name),
            None => &get_absolute_path(DEFAULT_CONFIG_PATH),
        };

        let default_config_file = File::open(config_file_name)?;
        load_and_process_config(default_config_file, node_command(), args)
    }

    pub fn load_and_process(args: Vec<String>) -> Result<Self, ConfigError> {
        Self::load_and_process_config_file(args, None)
    }
    pub fn load_and_process_file(
        args: Vec<String>,
        config_file_name: &str,
    ) -> Result<Self, ConfigError> {
        Self::load_and_process_config_file(args, Some(config_file_name))
    }
}

/// The command line interface of this node.
pub(crate) fn node_command() -> Command {
    Command::new("Sequencer")
        .version(VERSION_FULL)
        .about("A Starknet sequencer node written in Rust.")
}
