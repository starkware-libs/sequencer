use std::collections::BTreeMap;
use std::fs::File;
use std::path::Path;
use std::sync::LazyLock;
use std::vec::Vec;

use clap::Command;
use papyrus_config::dumping::{
    append_sub_config_name,
    ser_pointer_target_required_param,
    SerializeConfig,
};
use papyrus_config::loading::load_and_process_config;
use papyrus_config::validators::validate_ascii;
#[cfg(any(feature = "testing", test))]
use papyrus_config::SerializedContent;
use papyrus_config::{ConfigError, ParamPath, SerializationType, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;
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
pub const DEFAULT_CHAIN_ID: ChainId = ChainId::Mainnet;

// Required target parameters.
pub static REQUIRED_PARAM_CONFIG_POINTERS: LazyLock<Vec<(ParamPath, SerializedParam)>> =
    LazyLock::new(|| {
        vec![ser_pointer_target_required_param(
            "chain_id",
            SerializationType::String,
            "The chain to follow.",
        )]
    });

// Optional target parameters, i.e., target parameters with default values.
pub static DEFAULT_PARAM_CONFIG_POINTERS: LazyLock<Vec<(ParamPath, SerializedParam)>> =
    LazyLock::new(Vec::new);

// All target parameters.
pub static CONFIG_POINTERS: LazyLock<Vec<(ParamPath, SerializedParam)>> = LazyLock::new(|| {
    let mut combined = REQUIRED_PARAM_CONFIG_POINTERS.clone();
    combined.extend(DEFAULT_PARAM_CONFIG_POINTERS.clone());
    combined
});

// TODO(Tsabary): Bundle required config values in a struct, detailing whether they are pointer
// targets or not. Then, derive their values in the config (struct and pointers).
// Also, add functionality to derive them for testing.
// Creates a vector of strings with the command name and required parameters that can be used as
// arguments to load a config.
#[cfg(any(feature = "testing", test))]
pub fn create_test_config_load_args(pointers: &Vec<(ParamPath, SerializedParam)>) -> Vec<String> {
    let mut dummy_values = Vec::new();

    // Command name.
    dummy_values.push(node_command().to_string());

    // Iterate over required config parameters and add them as args with suitable arbitrary values.
    for (target_param, serialized_pointer) in pointers {
        // Param name.
        let required_param_name_as_arg = format!("--{}", target_param);
        dummy_values.push(required_param_name_as_arg);

        // Param value.
        let serialization_type = match &serialized_pointer.content {
            SerializedContent::ParamType(serialization_type) => serialization_type,
            _ => panic!("Required parameters have to be of type ParamType."),
        };
        let arbitrary_value = match serialization_type {
            SerializationType::Boolean => "false",
            SerializationType::Float => "15.2",
            SerializationType::NegativeInteger => "-30",
            SerializationType::PositiveInteger => "17",
            SerializationType::String => "ArbitraryString",
        }
        .to_string();
        dummy_values.push(arbitrary_value);
    }
    dummy_values
}

// TODO(yair): Make the GW and batcher execution config point to the same values.
/// The configurations of the various components of the node.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct SequencerNodeConfig {
    /// The [chain id](https://docs.rs/starknet_api/latest/starknet_api/core/struct.ChainId.html) of the Starknet network.
    #[validate(custom = "validate_ascii")]
    pub chain_id: ChainId,
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

impl Default for SequencerNodeConfig {
    fn default() -> Self {
        Self {
            chain_id: DEFAULT_CHAIN_ID,
            components: Default::default(),
            batcher_config: Default::default(),
            consensus_manager_config: Default::default(),
            gateway_config: Default::default(),
            http_server_config: Default::default(),
            rpc_state_reader_config: Default::default(),
            compiler_config: Default::default(),
            mempool_p2p_config: Default::default(),
            monitoring_endpoint_config: Default::default(),
        }
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
