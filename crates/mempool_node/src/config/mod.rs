#[cfg(test)]
mod config_test;

use std::collections::BTreeMap;
use std::fs::File;
use std::path::Path;
use std::sync::LazyLock;

use clap::Command;
use papyrus_config::dumping::{
    append_sub_config_name,
    ser_optional_sub_config,
    ser_param,
    ser_pointer_target_param,
    SerializeConfig,
};
use papyrus_config::loading::load_and_process_config;
use papyrus_config::validators::validate_ascii;
use papyrus_config::{ConfigError, ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;
use starknet_batcher::config::BatcherConfig;
use starknet_consensus_manager::config::ConsensusManagerConfig;
use starknet_gateway::config::{GatewayConfig, RpcStateReaderConfig};
use starknet_http_server::config::HttpServerConfig;
use starknet_mempool_infra::component_definitions::{
    LocalComponentCommunicationConfig,
    RemoteComponentCommunicationConfig,
};
use starknet_sierra_compile::config::SierraToCasmCompilationConfig;
use validator::{Validate, ValidationError};

use crate::version::VERSION_FULL;

// The path of the default configuration file, provided as part of the crate.
pub const DEFAULT_CONFIG_PATH: &str = "config/mempool/default_config.json";

// Configuration parameters that share the same value across multiple components.
type ConfigPointers = Vec<((ParamPath, SerializedParam), Vec<ParamPath>)>;
pub const DEFAULT_CHAIN_ID: ChainId = ChainId::Mainnet;
pub static CONFIG_POINTERS: LazyLock<ConfigPointers> = LazyLock::new(|| {
    vec![(
        ser_pointer_target_param("chain_id", &DEFAULT_CHAIN_ID, "The chain to follow."),
        vec![
            "batcher_config.storage.db_config.chain_id".to_owned(),
            "gateway_config.chain_info.chain_id".to_owned(),
        ],
    )]
});

// The configuration of the components.

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum ComponentExecutionMode {
    Local,
    Remote,
}
// TODO(Lev/Tsabary): When papyrus_config will support it, change to include communication config in
// the enum.

/// The single component configuration.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
#[validate(schema(function = "validate_single_component_config"))]
pub struct ComponentExecutionConfig {
    pub execute: bool,
    pub execution_mode: ComponentExecutionMode,
    pub local_config: Option<LocalComponentCommunicationConfig>,
    pub remote_config: Option<RemoteComponentCommunicationConfig>,
}

impl SerializeConfig for ComponentExecutionConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let config = BTreeMap::from_iter([
            ser_param(
                "execute",
                &self.execute,
                "The component execution flag.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "execution_mode",
                &self.execution_mode,
                "The component execution mode.",
                ParamPrivacyInput::Public,
            ),
        ]);
        vec![
            config,
            ser_optional_sub_config(&self.local_config, "local_config"),
            ser_optional_sub_config(&self.remote_config, "remote_config"),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

impl Default for ComponentExecutionConfig {
    fn default() -> Self {
        Self {
            execute: true,
            execution_mode: ComponentExecutionMode::Local,
            local_config: Some(LocalComponentCommunicationConfig::default()),
            remote_config: None,
        }
    }
}

/// Specific components default configurations.
impl ComponentExecutionConfig {
    pub fn gateway_default_config() -> Self {
        Self {
            execute: true,
            execution_mode: ComponentExecutionMode::Local,
            local_config: Some(LocalComponentCommunicationConfig::default()),
            remote_config: None,
        }
    }

    // TODO(Tsabary/Lev): There's a bug here: the http server component does not need a local nor a
    // remote config. However, the validation function requires that at least one of them is set. As
    // a workaround I've set the local one, but this should be addressed.
    pub fn http_server_default_config() -> Self {
        Self {
            execute: true,
            execution_mode: ComponentExecutionMode::Remote,
            local_config: None,
            remote_config: Some(RemoteComponentCommunicationConfig::default()),
        }
    }

    pub fn mempool_default_config() -> Self {
        Self {
            execute: true,
            execution_mode: ComponentExecutionMode::Local,
            local_config: Some(LocalComponentCommunicationConfig::default()),
            remote_config: None,
        }
    }

    pub fn batcher_default_config() -> Self {
        Self {
            execute: true,
            execution_mode: ComponentExecutionMode::Local,
            local_config: Some(LocalComponentCommunicationConfig::default()),
            remote_config: None,
        }
    }

    pub fn consensus_manager_default_config() -> Self {
        Self {
            execute: true,
            execution_mode: ComponentExecutionMode::Local,
            local_config: Some(LocalComponentCommunicationConfig::default()),
            remote_config: None,
        }
    }
}

pub fn validate_single_component_config(
    component_config: &ComponentExecutionConfig,
) -> Result<(), ValidationError> {
    let error_message =
        if component_config.local_config.is_some() && component_config.remote_config.is_some() {
            "Local config and Remote config are mutually exclusive, can't be both active."
        } else if component_config.execution_mode == ComponentExecutionMode::Local
            && component_config.local_config.is_none()
        {
            "Local communication config is missing."
        } else if component_config.execution_mode == ComponentExecutionMode::Remote
            && component_config.remote_config.is_none()
        {
            "Remote communication config is missing."
        } else {
            return Ok(());
        };

    let mut error = ValidationError::new("Invalid component configuration.");
    error.message = Some(error_message.into());
    Err(error)
}

/// The components configuration.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
#[validate(schema(function = "validate_components_config"))]
pub struct ComponentConfig {
    #[validate]
    pub batcher: ComponentExecutionConfig,
    #[validate]
    pub consensus_manager: ComponentExecutionConfig,
    #[validate]
    pub gateway: ComponentExecutionConfig,
    #[validate]
    pub http_server: ComponentExecutionConfig,
    #[validate]
    pub mempool: ComponentExecutionConfig,
}

impl Default for ComponentConfig {
    fn default() -> Self {
        Self {
            batcher: ComponentExecutionConfig::batcher_default_config(),
            consensus_manager: ComponentExecutionConfig::consensus_manager_default_config(),
            gateway: ComponentExecutionConfig::gateway_default_config(),
            http_server: ComponentExecutionConfig::http_server_default_config(),
            mempool: ComponentExecutionConfig::mempool_default_config(),
        }
    }
}

impl SerializeConfig for ComponentConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        #[allow(unused_mut)]
        let mut sub_configs = vec![
            append_sub_config_name(self.batcher.dump(), "batcher"),
            append_sub_config_name(self.consensus_manager.dump(), "consensus_manager"),
            append_sub_config_name(self.gateway.dump(), "gateway"),
            append_sub_config_name(self.http_server.dump(), "http_server"),
            append_sub_config_name(self.mempool.dump(), "mempool"),
        ];

        sub_configs.into_iter().flatten().collect()
    }
}

pub fn validate_components_config(components: &ComponentConfig) -> Result<(), ValidationError> {
    // TODO(Tsabary/Lev): We need to come up with a better mechanism for this validation, simply
    // listing all components and expecting one to remember adding a new component to this list does
    // not suffice.
    if components.gateway.execute
        || components.mempool.execute
        || components.batcher.execute
        || components.http_server.execute
        || components.consensus_manager.execute
    {
        return Ok(());
    }

    let mut error = ValidationError::new("Invalid components configuration.");
    error.message = Some("At least one component should be allowed to execute.".into());
    Err(error)
}

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
}

impl SerializeConfig for SequencerNodeConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        #[allow(unused_mut)]
        let mut sub_configs = vec![
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
            Some(file_name) => file_name,
            None => DEFAULT_CONFIG_PATH,
        };

        let default_config_file = File::open(Path::new(config_file_name))?;
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

// TODO(Tsabary): Rename the cli function.

/// The command line interface of this node.
pub fn node_command() -> Command {
    Command::new("Mempool")
        .version(VERSION_FULL)
        .about("Mempool is a Starknet mempool node written in Rust.")
}
