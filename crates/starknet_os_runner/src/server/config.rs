//! Configuration for the HTTP proving service.

use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;

use blockifier::blockifier::config::ContractClassManagerConfig;
use clap::Parser;
use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;

const DEFAULT_IP: IpAddr = IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0));
const DEFAULT_PORT: u16 = 3000;

/// Configuration for the HTTP proving service.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct ServiceConfig {
    /// Configuration for the contract class manager.
    pub contract_class_manager_config: ContractClassManagerConfig,
    /// Chain ID for transaction hash calculation.
    pub chain_id: ChainId,
    /// RPC node URL for fetching state.
    pub rpc_node_url: String,
    /// IP address to bind the server to.
    pub ip: IpAddr,
    /// Port to bind the server to.
    pub port: u16,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            contract_class_manager_config: ContractClassManagerConfig::default(),
            chain_id: ChainId::Mainnet,
            rpc_node_url: String::new(),
            ip: DEFAULT_IP,
            port: DEFAULT_PORT,
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
            config.chain_id = ChainId::from(chain_id);
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
