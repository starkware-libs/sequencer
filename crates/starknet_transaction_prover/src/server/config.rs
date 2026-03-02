//! Configuration for the proving service.

use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::str::FromStr;

use blockifier::bouncer::BouncerConfig;
use clap::Parser;
use serde::{Deserialize, Serialize};
use starknet_api::core::{ChainId, ContractAddress};
use tracing::info;

use crate::config::ProverConfig;
use crate::errors::ConfigError;
use crate::server::cors::normalize_cors_allow_origins;

#[cfg(test)]
#[path = "config_test.rs"]
mod config_test;

const DEFAULT_IP: IpAddr = IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0));
const DEFAULT_PORT: u16 = 3000;
const DEFAULT_MAX_CONCURRENT_REQUESTS: usize = 2;
const DEFAULT_MAX_CONNECTIONS: u32 = 10;

/// Transport mode for the JSON-RPC server.
#[derive(Clone, Debug)]
pub enum TransportMode {
    Http,
    Https { tls_cert_file: PathBuf, tls_key_file: PathBuf },
}

impl TransportMode {
    /// Constructs a `TransportMode` from optional cert and key paths.
    ///
    /// Returns `Http` when both are `None`, `Https` when both are `Some`, or an error when only
    /// one is provided.
    pub fn new(
        tls_cert_file: Option<PathBuf>,
        tls_key_file: Option<PathBuf>,
    ) -> Result<Self, ConfigError> {
        match (tls_cert_file, tls_key_file) {
            (None, None) => Ok(Self::Http),
            (Some(tls_cert_file), Some(tls_key_file)) => {
                Ok(Self::Https { tls_cert_file, tls_key_file })
            }
            (Some(_), None) => Err(ConfigError::IncompleteTlsConfig(
                "tls_cert_file is set but tls_key_file is missing".to_string(),
            )),
            (None, Some(_)) => Err(ConfigError::IncompleteTlsConfig(
                "tls_key_file is set but tls_cert_file is missing".to_string(),
            )),
        }
    }
}

/// Raw configuration as deserialized from JSON. TLS fields are optional and validated after CLI
/// overrides are applied.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
struct RawServiceConfig {
    #[serde(flatten)]
    prover_config: ProverConfig,
    ip: IpAddr,
    port: u16,
    max_concurrent_requests: usize,
    max_connections: u32,
    cors_allow_origin: Vec<String>,
    tls_cert_file: Option<PathBuf>,
    tls_key_file: Option<PathBuf>,
}

impl Default for RawServiceConfig {
    fn default() -> Self {
        Self {
            prover_config: ProverConfig::default(),
            ip: DEFAULT_IP,
            port: DEFAULT_PORT,
            max_concurrent_requests: DEFAULT_MAX_CONCURRENT_REQUESTS,
            max_connections: DEFAULT_MAX_CONNECTIONS,
            cors_allow_origin: Vec::new(),
            tls_cert_file: None,
            tls_key_file: None,
        }
    }
}

/// Configuration for the proving service.
#[derive(Clone, Debug)]
pub struct ServiceConfig {
    /// Configuration for the prover.
    pub prover_config: ProverConfig,
    /// IP address to bind the server to.
    pub ip: IpAddr,
    /// Port to bind the server to.
    pub port: u16,
    /// Maximum number of concurrent proving requests.
    pub max_concurrent_requests: usize,
    /// Maximum number of simultaneous JSON-RPC connections (safety net).
    pub max_connections: u32,
    /// List of allowed web origins (domains) that may call this HTTP service from a browser
    /// (CORS). Examples: `http://localhost:5173`, `https://app.example.com`, or `*` to allow any
    /// origin.
    pub cors_allow_origin: Vec<String>,
    /// Transport mode (HTTP or HTTPS with TLS).
    pub transport: TransportMode,
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
            serde_json::from_str::<RawServiceConfig>(&contents).map_err(|e| {
                ConfigError::ConfigFileError(format!(
                    "Failed to parse config file {}: {}",
                    config_file.display(),
                    e
                ))
            })?
        } else {
            RawServiceConfig::default()
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
                .map_err(|e| ConfigError::InvalidArgument(format!("Invalid IP address: {e}")))?;
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
        if let Some(tls_cert_file) = args.tls_cert_file {
            if Some(&tls_cert_file) != config.tls_cert_file.as_ref() {
                info!(
                    "CLI override: tls_cert_file: {:?} -> {:?}",
                    config.tls_cert_file, tls_cert_file
                );
                config.tls_cert_file = Some(tls_cert_file);
            }
        }
        if let Some(tls_key_file) = args.tls_key_file {
            if Some(&tls_key_file) != config.tls_key_file.as_ref() {
                info!(
                    "CLI override: tls_key_file: {:?} -> {:?}",
                    config.tls_key_file, tls_key_file
                );
                config.tls_key_file = Some(tls_key_file);
            }
        }

        if args.no_cors && !args.cors_allow_origin.is_empty() {
            return Err(ConfigError::InvalidArgument(
                "--no-cors and --cors-allow-origin are mutually exclusive".to_string(),
            ));
        }

        if args.no_cors {
            if !config.cors_allow_origin.is_empty() {
                info!(
                    "CLI override: cors_allow_origin: {:?} -> [] (--no-cors)",
                    config.cors_allow_origin
                );
            }
            config.cors_allow_origin = Vec::new();
        } else if !args.cors_allow_origin.is_empty() {
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
                ConfigError::InvalidArgument(format!("Invalid strk_fee_token_address: {e}"))
            })?;
            if Some(strk_fee_token_address) != config.prover_config.strk_fee_token_address {
                info!(
                    "CLI override: strk_fee_token_address: {:?} -> {:?}",
                    config.prover_config.strk_fee_token_address, strk_fee_token_address
                );
                config.prover_config.strk_fee_token_address = Some(strk_fee_token_address);
            }
        }

        if let Some(prefetch_state) = args.prefetch_state {
            if prefetch_state
                != config.prover_config.runner_config.virtual_block_executor_config.prefetch_state
            {
                info!(
                    "CLI override: prefetch_state: {} -> {}",
                    config.prover_config.runner_config.virtual_block_executor_config.prefetch_state,
                    prefetch_state
                );
                config.prover_config.runner_config.virtual_block_executor_config.prefetch_state =
                    prefetch_state;
            }
        }

        if let Some(bouncer_config_path) = args.bouncer_config_override {
            let bouncer_config_file_content = std::fs::read_to_string(&bouncer_config_path)
                .map_err(|e| {
                    ConfigError::ConfigFileError(format!(
                        "Failed to read bouncer config file {}: {}",
                        bouncer_config_path.display(),
                        e
                    ))
                })?;
            let bouncer_config: BouncerConfig = serde_json::from_str(&bouncer_config_file_content)
                .map_err(|e| {
                    ConfigError::ConfigFileError(format!(
                        "Failed to parse bouncer config file {}: {}",
                        bouncer_config_path.display(),
                        e
                    ))
                })?;
            info!("CLI override: bouncer_config from {}", bouncer_config_path.display());
            config.prover_config.runner_config.virtual_block_executor_config.bouncer_config =
                bouncer_config;
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
        let transport = TransportMode::new(config.tls_cert_file, config.tls_key_file)?;
        let cors_allow_origin = normalize_cors_allow_origins(config.cors_allow_origin)?;
        if cors_allow_origin == ["*"] {
            info!("CORS allow-origin configured as wildcard '*'.");
        }

        Ok(ServiceConfig {
            prover_config: config.prover_config,
            ip: config.ip,
            port: config.port,
            max_concurrent_requests: config.max_concurrent_requests,
            max_connections: config.max_connections,
            cors_allow_origin,
            transport,
        })
    }
}

/// CLI arguments for the proving service.
#[derive(Parser, Debug)]
#[command(name = "starknet-transaction-prover")]
#[command(about = "HTTP/HTTPS service for generating Starknet OS proofs", long_about = None)]
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

    /// Path to TLS certificate chain PEM file. Requires --tls-key-file.
    #[arg(long, value_name = "FILE")]
    pub tls_cert_file: Option<PathBuf>,

    /// Path to TLS private key PEM file. Requires --tls-cert-file.
    #[arg(long, value_name = "FILE")]
    pub tls_key_file: Option<PathBuf>,

    /// Override STRK fee token address (hex, e.g. for custom environments that share a chain ID).
    #[arg(long, value_name = "ADDRESS")]
    pub strk_fee_token_address: Option<String>,

    /// Prefetch state by simulating transactions before execution, reducing RPC calls during
    /// proving.
    #[arg(long)]
    pub prefetch_state: Option<bool>,

    /// Path to a JSON file with bouncer config overrides (block capacity limits).
    /// Client-side limits may differ from Starknet network limits.
    #[arg(long, value_name = "FILE")]
    pub bouncer_config_override: Option<PathBuf>,

    /// Disable CORS (clear any origins set in the config file).
    #[arg(long, conflicts_with = "cors_allow_origin")]
    pub no_cors: bool,

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
            - Origins are normalized and deduplicated.\n\n\
            Use --no-cors to explicitly disable CORS when a config file sets origins."
    )]
    pub cors_allow_origin: Vec<String>,
}
