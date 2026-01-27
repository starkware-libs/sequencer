//! Configuration for the HTTP proving service.
//!
//! This module provides both static configuration (set at startup) and dynamic
//! configuration (can be updated at runtime via a watch channel).

use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;

use blockifier::blockifier::config::ContractClassManagerConfig;
use clap::Parser;
use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;
use tokio::sync::watch;
use tracing::info;

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
            if rpc_url != config.rpc_node_url {
                info!("CLI override: rpc_node_url: {} -> {}", config.rpc_node_url, rpc_url);
                config.rpc_node_url = rpc_url;
            }
        }
        if let Some(chain_id) = args.chain_id {
            let new_chain_id = ChainId::from(chain_id.clone());
            if new_chain_id != config.chain_id {
                info!("CLI override: chain_id: {} -> {}", config.chain_id, new_chain_id);
                config.chain_id = new_chain_id;
            }
        }
        if let Some(port) = args.port {
            if port != config.port {
                info!("CLI override: port: {} -> {}", config.port, port);
                config.port = port;
            }
        }
        if let Some(ip) = args.ip {
            let new_ip: IpAddr = ip
                .parse()
                .map_err(|e| ConfigError::InvalidArgument(format!("Invalid IP address: {}", e)))?;
            if new_ip != config.ip {
                info!("CLI override: ip: {} -> {}", config.ip, new_ip);
                config.ip = new_ip;
            }
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

/// Dynamic configuration that can be updated at runtime.
///
/// Changes to dynamic configuration are propagated via a watch channel,
/// allowing for polling-free updates across all server components.
#[derive(Clone, Debug, Default)]
pub struct DynamicConfig {
    /// Whether the server should accept new proving requests.
    /// When set to `false`, new requests will be rejected with HTTP 503.
    pub accept_new_requests: bool,
}

impl DynamicConfig {
    /// Creates a new dynamic config with default values.
    pub fn new() -> Self {
        Self { accept_new_requests: true }
    }
}

/// Handle for sending dynamic configuration updates.
pub type DynamicConfigSender = watch::Sender<DynamicConfig>;

/// Handle for receiving dynamic configuration updates.
pub type DynamicConfigReceiver = watch::Receiver<DynamicConfig>;

/// Creates a new dynamic configuration channel.
///
/// Returns a sender and receiver pair. The sender can be used to update
/// the configuration, and the receiver can be cloned and shared with
/// components that need to react to configuration changes.
pub fn create_dynamic_config_channel() -> (DynamicConfigSender, DynamicConfigReceiver) {
    watch::channel(DynamicConfig::new())
}
