#[cfg(test)]
mod config_test;

use std::collections::BTreeMap;
use std::fs::File;
use std::path::Path;

use clap::Command;
use papyrus_config::dumping::{
    append_sub_config_name,
    ser_optional_sub_config,
    ser_param,
    SerializeConfig,
};
use papyrus_config::loading::load_and_process_config;
use papyrus_config::{ConfigError, ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_batcher::config::BatcherConfig;
use starknet_consensus_manager::config::ConsensusManagerConfig;
use starknet_gateway::config::{GatewayConfig, RpcStateReaderConfig};
use starknet_mempool_infra::component_definitions::{
    LocalComponentCommunicationConfig,
    RemoteComponentCommunicationConfig,
};
use starknet_sierra_compile::config::SierraToCasmCompilationConfig;
use validator::{Validate, ValidationError};

use crate::version::VERSION_FULL;

// The path of the default configuration file, provided as part of the crate.
pub const DEFAULT_CONFIG_PATH: &str = "config/mempool/default_config.json";

// The configuration of the components.

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum LocationType {
    Local,
    Remote,
}
// TODO(Lev/Tsabary): When papyrus_config will support it, change to include communication config in
// the enum.

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum ComponentType {
    // A component that perpetually runs upon start up, and does not receive requests from other
    // components. Example: an http server that listens to external requests.
    SelfInvokingComponent,
    // A component that runs upon receiving a request from another component. It cannot invoke
    // itself. Example: a mempool that receives transactions from the gateway.
    RequestServingComponent,
}

/// The single component configuration.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
#[validate(schema(function = "validate_single_component_config"))]
pub struct ComponentExecutionConfig {
    pub execute: bool,
    pub component_type: ComponentType,
    pub location: LocationType,
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
                "location",
                &self.location,
                "The component location.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "component_type",
                &self.component_type,
                "The component type.",
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
            location: LocationType::Local,
            component_type: ComponentType::RequestServingComponent,
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
            location: LocationType::Local,
            component_type: ComponentType::SelfInvokingComponent,
            local_config: Some(LocalComponentCommunicationConfig::default()),
            remote_config: None,
        }
    }

    pub fn mempool_default_config() -> Self {
        Self {
            execute: true,
            location: LocationType::Local,
            component_type: ComponentType::RequestServingComponent,
            local_config: Some(LocalComponentCommunicationConfig::default()),
            remote_config: None,
        }
    }

    pub fn batcher_default_config() -> Self {
        Self {
            execute: true,
            location: LocationType::Local,
            component_type: ComponentType::RequestServingComponent,
            local_config: Some(LocalComponentCommunicationConfig::default()),
            remote_config: None,
        }
    }

    pub fn consensus_manager_default_config() -> Self {
        Self {
            execute: true,
            location: LocationType::Local,
            component_type: ComponentType::RequestServingComponent,
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
        } else if component_config.location == LocationType::Local
            && component_config.local_config.is_none()
        {
            "Local communication config is missing."
        } else if component_config.location == LocationType::Remote
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
    pub mempool: ComponentExecutionConfig,
}

impl Default for ComponentConfig {
    fn default() -> Self {
        Self {
            batcher: ComponentExecutionConfig::batcher_default_config(),
            consensus_manager: ComponentExecutionConfig::consensus_manager_default_config(),
            gateway: ComponentExecutionConfig::gateway_default_config(),
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
            append_sub_config_name(self.mempool.dump(), "mempool"),
        ];

        sub_configs.into_iter().flatten().collect()
    }
}

pub fn validate_components_config(components: &ComponentConfig) -> Result<(), ValidationError> {
    if components.gateway.execute
        || components.mempool.execute
        || components.batcher.execute
        || components.consensus_manager.execute
    {
        return Ok(());
    }

    let mut error = ValidationError::new("Invalid components configuration.");
    error.message = Some("At least one component should be allowed to execute.".into());
    Err(error)
}

/// The configurations of the various components of the node.
#[derive(Debug, Deserialize, Default, Serialize, Clone, PartialEq, Validate)]
pub struct MempoolNodeConfig {
    #[validate]
    pub components: ComponentConfig,
    #[validate]
    pub batcher_config: BatcherConfig,
    #[validate]
    pub consensus_manager_config: ConsensusManagerConfig,
    #[validate]
    pub gateway_config: GatewayConfig,
    #[validate]
    pub rpc_state_reader_config: RpcStateReaderConfig,
    #[validate]
    pub compiler_config: SierraToCasmCompilationConfig,
}

impl SerializeConfig for MempoolNodeConfig {
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
            append_sub_config_name(self.rpc_state_reader_config.dump(), "rpc_state_reader_config"),
            append_sub_config_name(self.compiler_config.dump(), "compiler_config"),
        ];

        sub_configs.into_iter().flatten().collect()
    }
}

impl MempoolNodeConfig {
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

/// The command line interface of this node.
pub fn node_command() -> Command {
    Command::new("Mempool")
        .version(VERSION_FULL)
        .about("Mempool is a Starknet mempool node written in Rust.")
}
