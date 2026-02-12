//! Configuration for the HTTP proving service.

use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::str::FromStr;

use clap::Parser;
use serde::{Deserialize, Serialize};
use starknet_api::core::{ChainId, ContractAddress};
use tracing::info;

use crate::config::ProverConfig;

const DEFAULT_IP: IpAddr = IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0));
const DEFAULT_PORT: u16 = 3000;
const DEFAULT_MAX_CONCURRENT_REQUESTS: usize = 2;
const DEFAULT_MAX_CONNECTIONS: u32 = 10;

/// Configuration for the HTTP proving service.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct ServiceConfig {
    /// Configuration for the prover.
    #[serde(flatten)]
    pub prover_config: ProverConfig,
    /// IP address to bind the server to.
    pub ip: IpAddr,
    /// Port to bind the server to.
    pub port: u16,
    /// Maximum number of concurrent proving requests.
    pub max_concurrent_requests: usize,
    /// Maximum number of simultaneous JSON-RPC connections (safety net).
    pub max_connections: u32,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            prover_config: ProverConfig::default(),
            ip: DEFAULT_IP,
            port: DEFAULT_PORT,
            max_concurrent_requests: DEFAULT_MAX_CONCURRENT_REQUESTS,
            max_connections: DEFAULT_MAX_CONNECTIONS,
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
            if rpc_url != config.prover_config.rpc_node_url {
                info!(
                    "CLI override: rpc_node_url: {} -> {}",
                    config.prover_config.rpc_node_url, rpc_url
                );
                config.prover_config.rpc_node_url = rpc_url;
            }
        }
        if let Some(chain_id) = args.chain_id {
            let new_chain_id = ChainId::from(chain_id.clone());
            if new_chain_id != config.prover_config.chain_id {
                info!(
                    "CLI override: chain_id: {} -> {}",
                    config.prover_config.chain_id, new_chain_id
                );
                config.prover_config.chain_id = new_chain_id;
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
        if let Some(max) = args.max_concurrent_requests {
            if max != config.max_concurrent_requests {
                info!(
                    "CLI override: max_concurrent_requests: {} -> {}",
                    config.max_concurrent_requests, max
                );
                config.max_concurrent_requests = max;
            }
        }
        if let Some(max) = args.max_connections {
            if max != config.max_connections {
                info!("CLI override: max_connections: {} -> {}", config.max_connections, max);
                config.max_connections = max;
            }
        }

        if let Some(hex_str) = args.strk_fee_token_address {
            let strk_fee_token_address = ContractAddress::from_str(&hex_str).map_err(|e| {
                ConfigError::InvalidArgument(format!("Invalid strk_fee_token_address: {}", e))
            })?;
            if Some(strk_fee_token_address) != config.prover_config.strk_fee_token_address {
                info!(
                    "CLI override: strk_fee_token_address: {:?} -> {:?}",
                    config.prover_config.strk_fee_token_address, strk_fee_token_address
                );
                config.prover_config.strk_fee_token_address = Some(strk_fee_token_address);
            }
        }

        // Validate required fields.
        if config.prover_config.rpc_node_url.is_empty() {
            return Err(ConfigError::MissingRequiredField(
                "rpc_node_url is required (provide via --rpc-url or config file)".to_string(),
            ));
        }
        if config.max_concurrent_requests == 0 {
            return Err(ConfigError::InvalidArgument(
                "max_concurrent_requests must be at least 1".to_string(),
            ));
        }
        if config.max_connections == 0 {
            return Err(ConfigError::InvalidArgument(
                "max_connections must be at least 1".to_string(),
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

    /// Maximum number of concurrent proving requests (default: 1).
    #[arg(long, value_name = "N")]
    pub max_concurrent_requests: Option<usize>,

    /// Maximum number of simultaneous JSON-RPC connections (default: 10).
    #[arg(long, value_name = "N")]
    pub max_connections: Option<u32>,

    /// Override STRK fee token address (hex, e.g. for custom environments that share a chain ID).
    #[arg(long, value_name = "ADDRESS")]
    pub strk_fee_token_address: Option<String>,
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
