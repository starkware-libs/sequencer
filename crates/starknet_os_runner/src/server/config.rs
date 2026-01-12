//! Configuration for the HTTP proving service.

use std::net::IpAddr;
use std::path::PathBuf;

use blockifier::blockifier::config::ContractClassManagerConfig;
use clap::Parser;
use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;

/// Configuration for the HTTP proving service.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ServiceConfig {
    /// Configuration for the contract class manager.
    pub contract_class_manager_config: ContractClassManagerConfig,
    /// Chain ID for transaction hash calculation.
    pub chain_id: ChainId,
    /// RPC node URL for fetching state.
    pub rpc_node_url: String,
    /// IP address to bind the server to.
    #[serde(default = "default_ip")]
    pub ip: IpAddr,
    /// Port to bind the server to.
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_ip() -> IpAddr {
    "0.0.0.0".parse().unwrap()
}

fn default_port() -> u16 {
    3000
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            contract_class_manager_config: ContractClassManagerConfig::default(),
            chain_id: ChainId::Mainnet,
            rpc_node_url: String::new(),
            ip: default_ip(),
            port: default_port(),
        }
    }
}

impl ServiceConfig {
    /// Creates a ServiceConfig from CLI arguments.
    pub fn from_args(args: CliArgs) -> Result<Self, ConfigError> {
        let mut config = if let Some(config_file) = args.config_file {
            let contents = std::fs::read_to_string(&config_file).map_err(|e| {
                ConfigError::ConfigFileError(format!(
                    "Failed to read config file {}: {}",
                    config_file.display(),
                    e
                ))
            })?;
            serde_json::from_str(&contents).map_err(|e| {
                ConfigError::ConfigFileError(format!(
                    "Failed to parse config file {}: {}",
                    config_file.display(),
                    e
                ))
            })?
        } else {
            ServiceConfig::default()
        };

        // Override with CLI arguments if provided.
        if let Some(rpc_url) = args.rpc_url {
            config.rpc_node_url = rpc_url;
        }
        if let Some(chain_id) = args.chain_id {
            config.chain_id = parse_chain_id(&chain_id)?;
        }
        if let Some(port) = args.port {
            config.port = port;
        }
        if let Some(ip) = args.ip {
            config.ip = ip
                .parse()
                .map_err(|e| ConfigError::InvalidArgument(format!("Invalid IP address: {}", e)))?;
        }

        // Validate required fields.
        if config.rpc_node_url.is_empty() {
            return Err(ConfigError::MissingRequiredField(
                "rpc_node_url is required (provide via --rpc-url or config file)".to_string(),
            ));
        }

        Ok(config)
    }
}

/// Parses a chain ID string into a ChainId.
fn parse_chain_id(chain_id: &str) -> Result<ChainId, ConfigError> {
    match chain_id.to_lowercase().as_str() {
        "mainnet" | "sn_main" => Ok(ChainId::Mainnet),
        "sepolia" | "sn_sepolia" => Ok(ChainId::Sepolia),
        "integration-sepolia" | "sn_integration_sepolia" => Ok(ChainId::IntegrationSepolia),
        other => Ok(ChainId::Other(other.to_string())),
    }
}

/// CLI arguments for the proving service.
#[derive(Parser, Debug)]
#[command(name = "starknet-os-runner")]
#[command(about = "HTTP service for generating Starknet OS proofs", long_about = None)]
pub struct CliArgs {
    /// Path to JSON configuration file.
    #[arg(long, value_name = "FILE")]
    pub config_file: Option<PathBuf>,

    /// RPC node URL for fetching state.
    #[arg(long, value_name = "URL")]
    pub rpc_url: Option<String>,

    /// Chain ID (mainnet, sepolia, integration-sepolia, or custom).
    #[arg(long, value_name = "CHAIN_ID")]
    pub chain_id: Option<String>,

    /// Port to bind the server to.
    #[arg(long, value_name = "PORT")]
    pub port: Option<u16>,

    /// IP address to bind the server to.
    #[arg(long, value_name = "IP")]
    pub ip: Option<String>,
}

/// Errors that can occur during configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Configuration file error: {0}")]
    ConfigFileError(String),
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
    #[error("Missing required field: {0}")]
    MissingRequiredField(String),
}
