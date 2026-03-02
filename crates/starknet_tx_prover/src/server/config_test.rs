use std::io::Write;

use clap::Parser;
use rstest::rstest;
use tempfile::NamedTempFile;

use crate::errors::ConfigError;
use crate::server::config::{CliArgs, ServiceConfig};

/// RAII guard that removes an environment variable on drop, ensuring cleanup even on panic.
struct EnvGuard(&'static str);

impl EnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        // SAFETY: tests in this module are not concurrent with respect to the env vars they touch.
        unsafe { std::env::set_var(key, value) };
        Self(key)
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        // SAFETY: cleanup of env var set by the same test.
        unsafe { std::env::remove_var(self.0) };
    }
}

fn base_args() -> CliArgs {
    CliArgs {
        config_file: None,
        rpc_url: Some("http://localhost:9545".to_string()),
        chain_id: None,
        port: None,
        ip: None,
        max_concurrent_requests: None,
        max_connections: None,
        no_cors: false,
        cors_allow_origin: Vec::new(),
        strk_fee_token_address: None,
        prefetch_state: None,
    }
}

/// Happy-path cases: input origins -> expected normalized output.
#[rstest]
#[case::disabled(vec![], vec![])]
#[case::wildcard(vec!["*"], vec!["*"])]
#[case::multiple(
    vec!["https://example.com", "http://localhost:5173"],
    vec!["https://example.com", "http://localhost:5173"],
)]
#[case::normalizes_default_port(vec!["https://example.com:443/"], vec!["https://example.com"])]
#[case::deduplicates(
    vec!["https://example.com", "https://example.com:443/", "http://localhost:5173", "http://localhost:5173"],
    vec!["https://example.com", "http://localhost:5173"],
)]
#[case::wildcard_takes_precedence(
    vec!["https://example.com", "*", "http://localhost:5173"],
    vec!["*"],
)]
#[case::wildcard_ignores_invalid(vec!["*", "://invalid-origin"], vec!["*"])]
fn cors_allow_origin_valid_cases(#[case] input: Vec<&str>, #[case] expected: Vec<&str>) {
    let mut args = base_args();
    args.cors_allow_origin = input.into_iter().map(String::from).collect();

    let config = ServiceConfig::from_args(args).unwrap();

    assert_eq!(config.cors_allow_origin, expected);
}

#[test]
fn cors_allow_origin_rejects_path() {
    let mut args = base_args();
    args.cors_allow_origin = vec!["http://localhost:5173/path".to_string()];

    let error = ServiceConfig::from_args(args).unwrap_err();

    assert!(matches!(error, ConfigError::InvalidArgument(_)));
}

#[test]
fn cli_parses_repeated_cors_allow_origin_flags() {
    let args = CliArgs::parse_from([
        "starknet-tx-prover",
        "--rpc-url",
        "http://localhost:9545",
        "--cors-allow-origin",
        "http://localhost:5173",
        "--cors-allow-origin",
        "https://example.com",
    ]);

    assert_eq!(
        args.cors_allow_origin,
        vec!["http://localhost:5173".to_string(), "https://example.com".to_string()]
    );
}

#[test]
fn cors_allow_origin_rejects_non_array_in_config_file() {
    let mut config_file = NamedTempFile::new().unwrap();
    write!(config_file, "{{\"rpc_node_url\":\"http://localhost:9545\",\"cors_allow_origin\":\"http://localhost:5173\"}}")
        .unwrap();

    let args = CliArgs {
        config_file: Some(config_file.path().to_path_buf()),
        rpc_url: None,
        chain_id: None,
        port: None,
        ip: None,
        max_concurrent_requests: None,
        max_connections: None,
        no_cors: false,
        cors_allow_origin: Vec::new(),
        strk_fee_token_address: None,
        prefetch_state: None,
    };

    let error = ServiceConfig::from_args(args).unwrap_err();

    assert!(matches!(error, ConfigError::ConfigFileError(_)));
}

#[test]
fn env_var_sets_rpc_url() {
    let _guard = EnvGuard::set("RPC_URL", "http://env-provided:9545");

    let args = CliArgs::parse_from(["starknet-tx-prover"]);

    assert_eq!(args.rpc_url, Some("http://env-provided:9545".to_string()));
}

#[test]
fn cli_flag_overrides_env_var() {
    let _guard = EnvGuard::set("PORT", "5000");

    let args = CliArgs::parse_from(["starknet-tx-prover", "--port", "6000"]);

    assert_eq!(args.port, Some(6000));
}
