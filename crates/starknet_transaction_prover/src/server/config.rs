//! Configuration for the proving service.

use std::fmt::Display;
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::str::FromStr;

use blockifier::blockifier::config::ContractClassManagerConfig;
use blockifier::bouncer::BouncerConfig;
use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use starknet_api::core::{ChainId, ContractAddress};
use tracing::info;

use crate::config::ProverConfig;
use crate::errors::ConfigError;
use crate::running::runner::RunnerConfig;
use crate::running::storage_proofs::StorageProofConfig;
use crate::running::virtual_block_executor::RpcVirtualBlockExecutorConfig;
use crate::server::cors::normalize_cors_allow_origins;
use crate::server::log_redact::redact_url_host;

#[cfg(test)]
#[path = "config_test.rs"]
mod config_test;

const DEFAULT_IP: IpAddr = IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0));
const DEFAULT_PORT: u16 = 3000;
const DEFAULT_MAX_CONCURRENT_REQUESTS: usize = 2;
/// Default 8 so total in-flight (2 + 8) matches the default `max_connections` (10).
const DEFAULT_MAX_QUEUED_REQUESTS: usize = 8;
/// Backstop (≈ the client request timeout), not the primary shed — the queue length is.
const DEFAULT_QUEUE_WAIT_TIMEOUT_MILLIS: u64 = 30_000;
const DEFAULT_MAX_CONNECTIONS: u32 = 10;
const DEFAULT_COMPILED_CLASS_CACHE_SIZE: usize = 600;
/// 5 MiB — matches the convention used elsewhere in the sequencer.
pub(crate) const DEFAULT_MAX_REQUEST_BODY_SIZE: u32 = 5 * 1024 * 1024;
const DEFAULT_OHTTP_KEY_CACHE_MAX_AGE_SECS: u64 = 3600;
/// Default saturation window before `/health` returns 503. 10 seconds
/// matches "service is rejecting requests for a sustained period" without
/// flipping on a single in-flight burst.
const DEFAULT_HEALTH_MAX_SATURATED_MS: u64 = 10_000;

/// Transport mode for the JSON-RPC server.
#[derive(Clone, Debug)]
pub enum TransportMode {
    Http,
    Https { tls_cert_file: PathBuf, tls_key_file: PathBuf },
}

/// Output format for tracing log records.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "lowercase")]
pub enum LogFormat {
    #[default]
    Text,
    Json,
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

/// Raw configuration as deserialized from JSON. Flat structure that maps to user-facing config.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
struct RawServiceConfig {
    rpc_node_url: String,
    chain_id: ChainId,
    validate_zero_fee_fields: bool,
    strk_fee_token_address: Option<ContractAddress>,
    compiled_class_cache_size: usize,
    prefetch_state: bool,
    use_latest_versioned_constants: bool,
    ip: IpAddr,
    port: u16,
    max_concurrent_requests: usize,
    max_queued_requests: usize,
    queue_wait_timeout_millis: u64,
    max_connections: u32,
    cors_allow_origin: Vec<String>,
    tls_cert_file: Option<PathBuf>,
    tls_key_file: Option<PathBuf>,
    blocking_check_url: Option<String>,
    blocking_check_timeout_millis: u64,
    blocking_check_fail_open: bool,
    max_request_body_size: u32,
    ohttp_enabled: bool,
    ohttp_key_cache_max_age_secs: u64,
    health_max_saturated_ms: u64,
}

impl Default for RawServiceConfig {
    fn default() -> Self {
        Self {
            rpc_node_url: String::new(),
            chain_id: ChainId::Mainnet,
            validate_zero_fee_fields: true,
            strk_fee_token_address: None,
            compiled_class_cache_size: DEFAULT_COMPILED_CLASS_CACHE_SIZE,
            prefetch_state: false,
            use_latest_versioned_constants: true,
            ip: DEFAULT_IP,
            port: DEFAULT_PORT,
            max_concurrent_requests: DEFAULT_MAX_CONCURRENT_REQUESTS,
            max_queued_requests: DEFAULT_MAX_QUEUED_REQUESTS,
            queue_wait_timeout_millis: DEFAULT_QUEUE_WAIT_TIMEOUT_MILLIS,
            max_connections: DEFAULT_MAX_CONNECTIONS,
            cors_allow_origin: Vec::new(),
            tls_cert_file: None,
            tls_key_file: None,
            blocking_check_url: None,
            blocking_check_timeout_millis: 10000,
            blocking_check_fail_open: false,
            max_request_body_size: DEFAULT_MAX_REQUEST_BODY_SIZE,
            ohttp_enabled: false,
            ohttp_key_cache_max_age_secs: DEFAULT_OHTTP_KEY_CACHE_MAX_AGE_SECS,
            health_max_saturated_ms: DEFAULT_HEALTH_MAX_SATURATED_MS,
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
    /// Requests that may wait FIFO for a worker slot beyond `max_concurrent_requests`. When this
    /// buffer is full, requests are rejected with `-32005` (busy); `0` rejects the moment all
    /// workers are busy.
    pub max_queued_requests: usize,
    /// Backstop (ms) a queued request waits for a worker slot before a `-32005` rejection.
    pub queue_wait_timeout_millis: u64,
    /// Maximum number of simultaneous JSON-RPC connections (safety net).
    pub max_connections: u32,
    /// List of allowed web origins (domains) that may call this HTTP service from a browser
    /// (CORS). Examples: `http://localhost:5173`, `https://app.example.com`, or `*` to allow any
    /// origin.
    pub cors_allow_origin: Vec<String>,
    /// Transport mode (HTTP or HTTPS with TLS).
    pub transport: TransportMode,
    /// Maximum size of an incoming JSON-RPC request body in bytes.
    pub max_request_body_size: u32,
    /// Enable OHTTP (RFC 9458) envelope encryption. When true, the server accepts
    /// `message/ohttp-req` requests and serves key config at `GET /ohttp-keys`.
    /// Requires the `OHTTP_KEY` environment variable (hex-encoded 32-byte X25519 key).
    pub ohttp_enabled: bool,
    /// Cache-Control max-age for the `GET /ohttp-keys` response (seconds).
    pub ohttp_key_cache_max_age_secs: u64,
    /// Saturation window (milliseconds) before `/health` flips to 503. The
    /// service is "saturated" when it has been continuously rejecting
    /// proving requests due to the concurrency limit; once that has held
    /// for this many milliseconds, load balancers should drain the pod.
    pub health_max_saturated_ms: u64,
}

/// Applies an optional CLI override to a config field, logging `old -> new` when it changes.
/// Centralizes the repeated per-field override pattern used in `ServiceConfig::from_args`.
fn override_field<T: PartialEq + Display>(name: &str, current: &mut T, new: Option<T>) {
    if let Some(value) = new {
        if value != *current {
            info!("CLI override: {}: {} -> {}", name, *current, value);
            *current = value;
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
            if rpc_url != config.rpc_node_url {
                // `rpc_node_url` defaults to an empty string when no config
                // file is provided; treat empty as unset so the log doesn't
                // claim `<invalid url>` for the previous value.
                let previous = if config.rpc_node_url.is_empty() {
                    "<unset>".to_string()
                } else {
                    redact_url_host(&config.rpc_node_url)
                };
                info!("CLI override: rpc_node_url: {} -> {}", previous, redact_url_host(&rpc_url),);
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
        override_field("port", &mut config.port, args.port);
        if let Some(ip) = args.ip {
            let new_ip: IpAddr = ip
                .parse()
                .map_err(|e| ConfigError::InvalidArgument(format!("Invalid IP address: {e}")))?;
            if new_ip != config.ip {
                info!("CLI override: ip: {} -> {}", config.ip, new_ip);
                config.ip = new_ip;
            }
        }
        override_field(
            "max_concurrent_requests",
            &mut config.max_concurrent_requests,
            args.max_concurrent_requests,
        );
        override_field(
            "max_queued_requests",
            &mut config.max_queued_requests,
            args.max_queued_requests,
        );
        override_field(
            "queue_wait_timeout_millis",
            &mut config.queue_wait_timeout_millis,
            args.queue_wait_timeout_millis,
        );
        override_field("max_connections", &mut config.max_connections, args.max_connections);
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

        if args.skip_fee_field_validation && config.validate_zero_fee_fields {
            info!("CLI override: validate_zero_fee_fields: true -> false");
            config.validate_zero_fee_fields = false;
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
            if Some(strk_fee_token_address) != config.strk_fee_token_address {
                info!(
                    "CLI override: strk_fee_token_address: {:?} -> {:?}",
                    config.strk_fee_token_address, strk_fee_token_address
                );
                config.strk_fee_token_address = Some(strk_fee_token_address);
            }
        }

        override_field("prefetch_state", &mut config.prefetch_state, args.prefetch_state);

        override_field(
            "use_latest_versioned_constants",
            &mut config.use_latest_versioned_constants,
            args.use_latest_versioned_constants,
        );

        override_field(
            "compiled_class_cache_size",
            &mut config.compiled_class_cache_size,
            args.compiled_class_cache_size,
        );
        if let Some(url) = args.blocking_check_url {
            if Some(&url) != config.blocking_check_url.as_ref() {
                info!(
                    "CLI override: blocking_check_url: {} -> {}",
                    config
                        .blocking_check_url
                        .as_deref()
                        .map_or("<unset>".to_string(), redact_url_host),
                    redact_url_host(&url),
                );
                config.blocking_check_url = Some(url);
            }
        }
        override_field(
            "blocking_check_timeout_millis",
            &mut config.blocking_check_timeout_millis,
            args.blocking_check_timeout_millis,
        );
        override_field(
            "blocking_check_fail_open",
            &mut config.blocking_check_fail_open,
            args.blocking_check_fail_open,
        );

        // Validate blocking check URL early so an invalid value surfaces as a clean config error
        // instead of a panic at prover construction time.
        if let Some(url_str) = &config.blocking_check_url {
            url::Url::parse(url_str).map_err(|e| {
                ConfigError::InvalidArgument(format!("Invalid blocking_check_url: {e}"))
            })?;
        }
        override_field(
            "max_request_body_size",
            &mut config.max_request_body_size,
            args.max_request_body_size,
        );

        if args.ohttp_enabled && !config.ohttp_enabled {
            info!("CLI override: ohttp_enabled: false -> true");
            config.ohttp_enabled = true;
        }
        override_field(
            "ohttp_key_cache_max_age_secs",
            &mut config.ohttp_key_cache_max_age_secs,
            args.ohttp_key_cache_max_age_secs,
        );
        override_field(
            "health_max_saturated_ms",
            &mut config.health_max_saturated_ms,
            args.health_max_saturated_ms,
        );

        // Validate required fields.
        if config.rpc_node_url.is_empty() {
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
        // Waiting requests hold a connection, so queue depth is capped by max_connections.
        let max_in_flight =
            config.max_concurrent_requests.saturating_add(config.max_queued_requests);
        if max_in_flight > usize::try_from(config.max_connections).unwrap_or(usize::MAX) {
            info!(
                "max_concurrent_requests ({}) + max_queued_requests ({}) exceeds max_connections \
                 ({}); queue depth is effectively capped by max_connections",
                config.max_concurrent_requests, config.max_queued_requests, config.max_connections,
            );
        }
        if config.max_request_body_size == 0 {
            return Err(ConfigError::InvalidArgument(
                "max_request_body_size must be at least 1".to_string(),
            ));
        }
        let transport = TransportMode::new(config.tls_cert_file, config.tls_key_file)?;
        let cors_allow_origin = normalize_cors_allow_origins(config.cors_allow_origin)?;
        if cors_allow_origin == ["*"] {
            info!("CORS allow-origin configured as wildcard '*'.");
        }

        // Load bouncer config from CLI-specified file or fall back to embedded resource.
        let bouncer_config: BouncerConfig = if let Some(path) = args.bouncer_config_override {
            let contents = std::fs::read_to_string(&path).map_err(|e| {
                ConfigError::ConfigFileError(format!(
                    "Failed to read bouncer config file {}: {}",
                    path.display(),
                    e
                ))
            })?;
            info!("Loading bouncer config from {}", path.display());
            serde_json::from_str(&contents).map_err(|e| {
                ConfigError::ConfigFileError(format!(
                    "Failed to parse bouncer config file {}: {}",
                    path.display(),
                    e
                ))
            })?
        } else {
            serde_json::from_str(include_str!("../../resources/bouncer_config.json"))
                .expect("embedded bouncer_config.json is invalid")
        };

        // Build nested structs from flat config.
        let prover_config = ProverConfig {
            contract_class_manager_config: ContractClassManagerConfig {
                contract_cache_size: config.compiled_class_cache_size,
                ..ContractClassManagerConfig::default()
            },
            chain_id: config.chain_id,
            rpc_node_url: config.rpc_node_url,
            runner_config: RunnerConfig {
                storage_proof_config: StorageProofConfig::default(),
                virtual_block_executor_config: RpcVirtualBlockExecutorConfig {
                    prefetch_state: config.prefetch_state,
                    bouncer_config,
                    use_latest_versioned_constants: config.use_latest_versioned_constants,
                },
            },
            strk_fee_token_address: config.strk_fee_token_address,
            validate_zero_fee_fields: config.validate_zero_fee_fields,
            blocking_check_url: config.blocking_check_url,
            blocking_check_timeout_millis: config.blocking_check_timeout_millis,
            blocking_check_fail_open: config.blocking_check_fail_open,
        };

        Ok(ServiceConfig {
            prover_config,
            ip: config.ip,
            port: config.port,
            max_concurrent_requests: config.max_concurrent_requests,
            max_queued_requests: config.max_queued_requests,
            queue_wait_timeout_millis: config.queue_wait_timeout_millis,
            max_connections: config.max_connections,
            cors_allow_origin,
            transport,
            max_request_body_size: config.max_request_body_size,
            ohttp_enabled: config.ohttp_enabled,
            ohttp_key_cache_max_age_secs: config.ohttp_key_cache_max_age_secs,
            health_max_saturated_ms: config.health_max_saturated_ms,
        })
    }

    /// Logs the fully resolved configuration once at startup, so an operator can see the
    /// effective values (after file + env + CLI merge) in the logs.
    ///
    /// Secrets are redacted: the RPC and blocking-check URLs are reduced to host-only, dropping
    /// any userinfo/path/query; TLS key/cert paths are never logged (only the transport scheme).
    /// Every other field is an operational knob that is safe to record.
    pub fn log_startup_summary(&self) {
        let transport = match self.transport {
            TransportMode::Http => "http",
            TransportMode::Https { .. } => "https",
        };
        let blocking_check_host =
            self.prover_config.blocking_check_url.as_deref().map(redact_url_host);
        info!(
            event = "config_resolved",
            ip = %self.ip,
            port = self.port,
            transport,
            max_concurrent_requests = self.max_concurrent_requests,
            max_queued_requests = self.max_queued_requests,
            queue_wait_timeout_millis = self.queue_wait_timeout_millis,
            max_connections = self.max_connections,
            max_request_body_size = self.max_request_body_size,
            cors_allow_origin = ?self.cors_allow_origin,
            ohttp_enabled = self.ohttp_enabled,
            ohttp_key_cache_max_age_secs = self.ohttp_key_cache_max_age_secs,
            health_max_saturated_ms = self.health_max_saturated_ms,
            chain_id = %self.prover_config.chain_id,
            rpc_node_host = %redact_url_host(&self.prover_config.rpc_node_url),
            strk_fee_token_address = ?self.prover_config.strk_fee_token_address,
            validate_zero_fee_fields = self.prover_config.validate_zero_fee_fields,
            blocking_check_enabled = self.prover_config.blocking_check_url.is_some(),
            blocking_check_host = ?blocking_check_host,
            blocking_check_timeout_millis = self.prover_config.blocking_check_timeout_millis,
            blocking_check_fail_open = self.prover_config.blocking_check_fail_open,
            "Resolved service configuration."
        );
    }
}

/// CLI arguments for the proving service.
#[derive(Parser, Debug)]
#[command(name = "starknet-transaction-prover")]
#[command(about = "HTTP/HTTPS service for generating Starknet OS proofs", long_about = None)]
pub struct CliArgs {
    /// Path to JSON configuration file.
    #[arg(long, value_name = "FILE", env = "CONFIG_FILE")]
    pub config_file: Option<PathBuf>,

    /// RPC node URL for fetching state.
    #[arg(long, value_name = "URL", env = "RPC_URL")]
    pub rpc_url: Option<String>,

    /// Chain ID (mainnet, sepolia, integration-sepolia, or custom).
    #[arg(long, value_name = "CHAIN_ID", env = "CHAIN_ID")]
    pub chain_id: Option<String>,

    /// Port to bind the server to.
    #[arg(long, value_name = "PORT", env = "PROVER_PORT")]
    pub port: Option<u16>,

    /// IP address to bind the server to.
    #[arg(long, value_name = "IP", env = "PROVER_IP")]
    pub ip: Option<String>,

    /// Maximum number of concurrent proving requests (default: 1).
    #[arg(long, value_name = "N", env = "MAX_CONCURRENT_REQUESTS")]
    pub max_concurrent_requests: Option<usize>,

    /// Requests that may wait for a worker slot beyond --max-concurrent-requests (default: 8; 0 =
    /// reject immediately).
    #[arg(long, value_name = "N", env = "MAX_QUEUED_REQUESTS")]
    pub max_queued_requests: Option<usize>,

    /// Backstop ms a queued request waits for a slot before a busy rejection (default: 30000).
    #[arg(long, value_name = "MILLIS", env = "QUEUE_WAIT_TIMEOUT_MILLIS")]
    pub queue_wait_timeout_millis: Option<u64>,

    /// Maximum number of simultaneous JSON-RPC connections (default: 10).
    #[arg(long, value_name = "N", env = "MAX_CONNECTIONS")]
    pub max_connections: Option<u32>,

    /// Path to TLS certificate chain PEM file. Requires --tls-key-file.
    #[arg(long, value_name = "FILE", env = "TLS_CERT_FILE")]
    pub tls_cert_file: Option<PathBuf>,

    /// Path to TLS private key PEM file. Requires --tls-cert-file.
    #[arg(long, value_name = "FILE", env = "TLS_KEY_FILE")]
    pub tls_key_file: Option<PathBuf>,

    /// Override STRK fee token address (hex, e.g. for custom environments that share a chain ID).
    #[arg(long, value_name = "ADDRESS", env = "STRK_FEE_TOKEN_ADDRESS")]
    pub strk_fee_token_address: Option<String>,

    /// Prefetch state by simulating transactions before execution, reducing RPC calls during
    /// proving.
    #[arg(long, env = "PREFETCH_STATE")]
    pub prefetch_state: Option<bool>,

    /// Use the latest versioned constants instead of the ones matching the block's version.
    /// The OS always runs with the latest constants, so this should match (default: true).
    #[arg(long, env = "USE_LATEST_VERSIONED_CONSTANTS")]
    pub use_latest_versioned_constants: Option<bool>,

    /// Skip validation that fee-related fields (resource bounds, tip) are zero.
    #[arg(long, env = "SKIP_FEE_FIELD_VALIDATION")]
    pub skip_fee_field_validation: bool,

    /// Disable CORS (clear any origins set in the config file).
    #[arg(long, conflicts_with = "cors_allow_origin")]
    pub no_cors: bool,

    /// CORS allow-origin values (`*` or one or more origins such as `http://localhost:5173`).
    #[arg(
        long,
        value_name = "ORIGIN",
        env = "CORS_ALLOW_ORIGIN",
        value_delimiter = ',',
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

    /// Number of compiled contract classes to cache in memory.
    #[arg(long, value_name = "N", env = "COMPILED_CLASS_CACHE_SIZE")]
    pub compiled_class_cache_size: Option<usize>,

    /// Maximum size of an incoming JSON-RPC request body in bytes (default: 5 MiB).
    #[arg(long, value_name = "BYTES", env = "MAX_REQUEST_BODY_SIZE")]
    pub max_request_body_size: Option<u32>,

    /// Enable OHTTP (RFC 9458) envelope encryption. Requires the `OHTTP_KEY` env var.
    #[arg(long, env = "OHTTP_ENABLED")]
    pub ohttp_enabled: bool,

    /// Cache-Control max-age (seconds) for the `GET /ohttp-keys` response (default: 3600).
    #[arg(long, value_name = "SECS", env = "OHTTP_KEY_CACHE_MAX_AGE_SECS")]
    pub ohttp_key_cache_max_age_secs: Option<u64>,

    /// Log output format. Use `json` in production so log aggregators parse fields directly.
    #[arg(long, value_enum, value_name = "FORMAT", env = "LOG_FORMAT", default_value_t = LogFormat::Text)]
    pub log_format: LogFormat,

    /// Saturation window (milliseconds) before `/health` returns 503
    /// (default: 10000). The service is "saturated" when it has been
    /// continuously rejecting proving requests due to the concurrency
    /// limit; once that has held for this many milliseconds, load
    /// balancers should drain the pod.
    #[arg(long, value_name = "MILLIS", env = "HEALTH_MAX_SATURATED_MS")]
    pub health_max_saturated_ms: Option<u64>,

    /// Hidden escape hatch: override the embedded bouncer config (block capacity limits) with a
    /// custom JSON file. Not advertised because the embedded defaults are tuned for this prover
    /// (including high `l1_gas` / `message_segment_length`: virtual OS output is not L1-bound; it
    /// is carried on L2 in `proof_fact`). Exposed for debugging and testing without rebuilding.
    #[arg(long, value_name = "FILE", env = "BOUNCER_CONFIG_OVERRIDE", hide = true)]
    pub bouncer_config_override: Option<PathBuf>,

    /// URL of the external blocking check JSON-RPC service (HTTPS with self-signed cert
    /// supported).
    #[arg(long, value_name = "URL", env = "BLOCKING_CHECK_URL")]
    pub blocking_check_url: Option<String>,

    /// Milliseconds to wait for the blocking check response before applying the
    /// fail-open/fail-close policy (default: 10000).
    #[arg(long, value_name = "MILLIS", env = "BLOCKING_CHECK_TIMEOUT_MILLIS")]
    pub blocking_check_timeout_millis: Option<u64>,

    /// Fail-open when blocking check is inconclusive (default: false). Set to false for
    /// fail-close.
    #[arg(long, env = "BLOCKING_CHECK_FAIL_OPEN")]
    pub blocking_check_fail_open: Option<bool>,
}
