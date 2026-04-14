use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use clap::Parser;
use rstest::rstest;
use tempfile::NamedTempFile;

use crate::errors::ConfigError;
use crate::server::config::{CliArgs, ServiceConfig, TransportMode};

/// Mutex that serializes tests which modify environment variables.
static ENV_MUTEX: Mutex<()> = Mutex::new(());

/// RAII guard that holds the env mutex and removes the environment variable on drop.
struct EnvGuard {
    key: &'static str,
    _lock: MutexGuard<'static, ()>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let lock = ENV_MUTEX.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        // SAFETY: we hold ENV_MUTEX, so no other env-mutating test runs concurrently.
        unsafe { std::env::set_var(key, value) };
        Self { key, _lock: lock }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        // SAFETY: we still hold ENV_MUTEX (dropped after this field).
        unsafe { std::env::remove_var(self.key) };
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
        tls_cert_file: None,
        tls_key_file: None,
        skip_fee_field_validation: false,
        no_cors: false,
        cors_allow_origin: Vec::new(),
        strk_fee_token_address: None,
        prefetch_state: None,
        use_latest_versioned_constants: None,
        compiled_class_cache_size: None,
        bouncer_config_override: None,
        blocking_check_url: None,
        blocking_check_timeout_secs: None,
        blocking_check_fail_open: None,
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
        "starknet-transaction-prover",
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
        tls_cert_file: None,
        tls_key_file: None,
        skip_fee_field_validation: false,
        no_cors: false,
        cors_allow_origin: Vec::new(),
        strk_fee_token_address: None,
        prefetch_state: None,
        use_latest_versioned_constants: None,
        compiled_class_cache_size: None,
        bouncer_config_override: None,
        blocking_check_url: None,
        blocking_check_timeout_secs: None,
        blocking_check_fail_open: None,
    };

    let error = ServiceConfig::from_args(args).unwrap_err();

    assert!(matches!(error, ConfigError::ConfigFileError(_)));
}

/// TLS configuration validation: partial TLS config is rejected, complete config is accepted.
#[rstest]
#[case::cert_without_key(
    Some("cert.pem".into()),
    None,
    None,
    true
)]
#[case::key_without_cert(
    None,
    Some("key.pem".into()),
    None,
    true
)]
#[case::both_provided(
    Some("cert.pem".into()),
    Some("key.pem".into()),
    None,
    false
)]
#[case::neither_provided(None, None, None, false)]
fn tls_config_validation(
    #[case] tls_cert_file: Option<PathBuf>,
    #[case] tls_key_file: Option<PathBuf>,
    #[case] config_file: Option<PathBuf>,
    #[case] expect_error: bool,
) {
    let mut args = base_args();
    args.tls_cert_file = tls_cert_file;
    args.tls_key_file = tls_key_file;
    args.config_file = config_file;

    let result = ServiceConfig::from_args(args);

    if expect_error {
        assert!(matches!(result.unwrap_err(), ConfigError::IncompleteTlsConfig(_)));
    } else {
        result.unwrap();
    }
}

#[test]
fn tls_transport_mode_is_https_when_both_files_provided() {
    let mut args = base_args();
    args.tls_cert_file = Some("cert.pem".into());
    args.tls_key_file = Some("key.pem".into());

    let config = ServiceConfig::from_args(args).unwrap();

    match &config.transport {
        TransportMode::Https { tls_cert_file, tls_key_file } => {
            assert_eq!(tls_cert_file, &PathBuf::from("cert.pem"));
            assert_eq!(tls_key_file, &PathBuf::from("key.pem"));
        }
        TransportMode::Http => panic!("Expected Https transport mode"),
    }
}

#[test]
fn tls_transport_mode_is_http_when_no_tls_files() {
    let args = base_args();

    let config = ServiceConfig::from_args(args).unwrap();

    assert!(matches!(config.transport, TransportMode::Http));
}

#[test]
fn config_file_tls_cert_completed_by_cli_key() {
    let mut config_file = NamedTempFile::new().unwrap();
    write!(
        config_file,
        r#"{{"rpc_node_url":"http://localhost:9545","tls_cert_file":"cert.pem"}}"#,
    )
    .unwrap();

    let mut args = base_args();
    args.config_file = Some(config_file.path().to_path_buf());
    args.rpc_url = None;
    args.tls_key_file = Some("key.pem".into());

    let config = ServiceConfig::from_args(args).unwrap();

    match &config.transport {
        TransportMode::Https { tls_cert_file, tls_key_file } => {
            assert_eq!(tls_cert_file, &PathBuf::from("cert.pem"));
            assert_eq!(tls_key_file, &PathBuf::from("key.pem"));
        }
        TransportMode::Http => panic!("Expected Https transport mode"),
    }
}

#[test]
fn config_file_tls_key_completed_by_cli_cert() {
    let mut config_file = NamedTempFile::new().unwrap();
    write!(config_file, r#"{{"rpc_node_url":"http://localhost:9545","tls_key_file":"key.pem"}}"#,)
        .unwrap();

    let mut args = base_args();
    args.config_file = Some(config_file.path().to_path_buf());
    args.rpc_url = None;
    args.tls_cert_file = Some("cert.pem".into());

    let config = ServiceConfig::from_args(args).unwrap();

    match &config.transport {
        TransportMode::Https { tls_cert_file, tls_key_file } => {
            assert_eq!(tls_cert_file, &PathBuf::from("cert.pem"));
            assert_eq!(tls_key_file, &PathBuf::from("key.pem"));
        }
        TransportMode::Http => panic!("Expected Https transport mode"),
    }
}

#[test]
fn env_var_sets_rpc_url() {
    let _guard = EnvGuard::set("RPC_URL", "http://env-provided:9545");

    let args = CliArgs::parse_from(["starknet-transaction-prover"]);

    assert_eq!(args.rpc_url, Some("http://env-provided:9545".to_string()));
}

#[test]
fn cli_flag_overrides_env_var() {
    let _guard = EnvGuard::set("PROVER_PORT", "5000");

    let args = CliArgs::parse_from(["starknet-transaction-prover", "--port", "6000"]);

    assert_eq!(args.port, Some(6000));
}

#[test]
fn env_var_sets_tls_cert_file() {
    let _guard = EnvGuard::set("TLS_CERT_FILE", "/etc/ssl/cert.pem");

    let args = CliArgs::parse_from(["starknet-transaction-prover"]);

    assert_eq!(args.tls_cert_file, Some(PathBuf::from("/etc/ssl/cert.pem")));
}

#[test]
fn env_var_sets_tls_key_file() {
    let _guard = EnvGuard::set("TLS_KEY_FILE", "/etc/ssl/key.pem");

    let args = CliArgs::parse_from(["starknet-transaction-prover"]);

    assert_eq!(args.tls_key_file, Some(PathBuf::from("/etc/ssl/key.pem")));
}
