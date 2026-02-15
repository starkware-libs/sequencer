//! Configuration for the HTTP proving service.

use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::str::FromStr;

use clap::Parser;
use serde::{Deserialize, Serialize};
use starknet_api::core::{ChainId, ContractAddress};
use tracing::info;
use url::Url;

use crate::config::ProverConfig;

#[cfg(test)]
#[path = "config_test.rs"]
mod config_test;

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
    /// CORS allow-origin values for HTTP responses (for example `http://localhost:5173` or `*`).
    pub cors_allow_origin: Vec<String>,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            prover_config: ProverConfig::default(),
            ip: DEFAULT_IP,
            port: DEFAULT_PORT,
            max_concurrent_requests: DEFAULT_MAX_CONCURRENT_REQUESTS,
            max_connections: DEFAULT_MAX_CONNECTIONS,
            cors_allow_origin: Vec::new(),
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
        if !args.cors_allow_origin.is_empty() {
            if args.cors_allow_origin != config.cors_allow_origin {
                info!(
                    "CLI override: cors_allow_origin: {:?} -> {:?}",
                    config.cors_allow_origin, args.cors_allow_origin
                );
            }
            config.cors_allow_origin = args.cors_allow_origin;
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
        config.cors_allow_origin = normalize_cors_allow_origins(config.cors_allow_origin)?;
        if config.cors_allow_origin == ["*"] {
            info!("CORS allow-origin configured as wildcard '*'.");
        }

        Ok(config)
    }
}

fn normalize_cors_allow_origins(
    cors_allow_origins: Vec<String>,
) -> Result<Vec<String>, ConfigError> {
    if cors_allow_origins.iter().any(|origin| origin == "*") {
        return Ok(vec!["*".to_string()]);
    }

    let mut normalized_allow_origins = Vec::new();

    for cors_allow_origin in cors_allow_origins {
        let normalized_allow_origin = normalize_cors_allow_origin(&cors_allow_origin)?;
        if !normalized_allow_origins.contains(&normalized_allow_origin) {
            normalized_allow_origins.push(normalized_allow_origin);
        }
    }

    Ok(normalized_allow_origins)
}

fn normalize_cors_allow_origin(cors_allow_origin: &str) -> Result<String, ConfigError> {
    let parsed = Url::parse(cors_allow_origin).map_err(|e| {
        ConfigError::InvalidArgument(format!(
            "Invalid cors_allow_origin '{cors_allow_origin}': {e}"
        ))
    })?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(ConfigError::InvalidArgument(format!(
            "Invalid cors_allow_origin '{cors_allow_origin}': only http:// and https:// are \
             supported."
        )));
    }
    if parsed.host().is_none() {
        return Err(ConfigError::InvalidArgument(format!(
            "Invalid cors_allow_origin '{cors_allow_origin}': host is required."
        )));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(ConfigError::InvalidArgument(format!(
            "Invalid cors_allow_origin '{cors_allow_origin}': userinfo is not supported."
        )));
    }
    if parsed.path() != "/" || parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(ConfigError::InvalidArgument(format!(
            "Invalid cors_allow_origin '{cors_allow_origin}': must be '*' or \
             '<scheme>://<host>[:port]' without a path, query, or fragment."
        )));
    }

    let host = parsed.host().ok_or_else(|| {
        ConfigError::InvalidArgument(format!(
            "Invalid cors_allow_origin '{cors_allow_origin}': host is required."
        ))
    })?;
    let mut normalized = format!("{}://{}", parsed.scheme(), host);

    if let Some(port) = parsed.port() {
        let is_default_port = (parsed.scheme() == "http" && port == 80)
            || (parsed.scheme() == "https" && port == 443);
        if !is_default_port {
            normalized.push(':');
            normalized.push_str(&port.to_string());
        }
    }

    Ok(normalized)
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

    /// CORS allow-origin values (`*` or one or more origins such as `http://localhost:5173`).
    #[arg(
        long,
        value_name = "ORIGIN",
        long_help = "CORS allow-origin values ('*' or one or more origins).\n\n\
            Repeat the flag for multiple origins:\n  \
            --cors-allow-origin http://localhost:5173 \\\n  \
            --cors-allow-origin https://app.example.com\n\n\
            Rules:\n  \
            - Omitted or empty: CORS is disabled (no Access-Control-Allow-Origin header).\n  \
            - '*': allow all origins (wildcard mode).\n  \
            - If '*' appears alongside other values, wildcard mode is used and the rest are \
            ignored.\n  \
            - Only http:// and https:// origins are accepted.\n  \
            - Paths, query strings, fragments, and userinfo are rejected.\n  \
            - Origins are normalized and deduplicated."
    )]
    pub cors_allow_origin: Vec<String>,
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
