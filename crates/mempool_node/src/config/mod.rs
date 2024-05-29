#[cfg(test)]
mod config_test;

use std::collections::BTreeMap;
use std::fs::File;
use std::path::Path;

use clap::Command;
use papyrus_config::dumping::{append_sub_config_name, ser_param, SerializeConfig};
use papyrus_config::loading::load_and_process_config;
use papyrus_config::{ConfigError, ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_gateway::config::{GatewayConfig, RpcStateReaderConfig};
use validator::{Validate, ValidationError};

use crate::version::VERSION_FULL;

// The path of the default configuration file, provided as part of the crate.
pub const DEFAULT_CONFIG_PATH: &str = "config/mempool_default_config.json";

/// The single crate configuration.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct ComponentExecutionConfig {
    pub execute: bool,
}

impl SerializeConfig for ComponentExecutionConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([ser_param(
            "execute",
            &self.execute,
            "The component execution flag.",
            ParamPrivacyInput::Public,
        )])
    }
}

impl Default for ComponentExecutionConfig {
    fn default() -> Self {
        Self { execute: true }
    }
}

/// The components configuration.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
#[validate(schema(function = "validate_components_config"))]
pub struct ComponentConfig {
    pub gateway_component: ComponentExecutionConfig,
    pub mempool_component: ComponentExecutionConfig,
}

impl SerializeConfig for ComponentConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        #[allow(unused_mut)]
        let mut sub_configs = vec![
            append_sub_config_name(self.gateway_component.dump(), "gateway_component"),
            append_sub_config_name(self.mempool_component.dump(), "mempool_component"),
        ];

        sub_configs.into_iter().flatten().collect()
    }
}

pub fn validate_components_config(components: &ComponentConfig) -> Result<(), ValidationError> {
    if components.gateway_component.execute || components.mempool_component.execute {
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
    pub gateway_config: GatewayConfig,
    #[validate]
    pub rpc_state_reader_config: RpcStateReaderConfig,
}

impl SerializeConfig for MempoolNodeConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        #[allow(unused_mut)]
        let mut sub_configs = vec![
            append_sub_config_name(self.components.dump(), "components"),
            append_sub_config_name(self.gateway_config.dump(), "gateway_config"),
            append_sub_config_name(self.rpc_state_reader_config.dump(), "rpc_state_reader_config"),
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
        .about("Mempool is a StarkNet mempool node written in Rust.")
}
