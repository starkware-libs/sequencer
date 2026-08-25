use std::io::Write;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use assert_matches::assert_matches;
use clap::Parser;
use rstest::rstest;
use tempfile::NamedTempFile;

use crate::errors::ConfigError;
use crate::server::config::{CliArgs, LogFormat, ServiceConfig, TransportMode};

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
        max_queued_requests: None,
        queue_wait_timeout_millis: None,
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
        blocking_check_timeout_millis: None,
        blocking_check_fail_open: None,
        max_request_body_size: None,
        ohttp_enabled: false,
        ohttp_key_cache_max_age_secs: None,
        log_format: LogFormat::Text,
    }
}

#[test]
fn rejects_request_limits_exceeding_semaphore_capacity() {
    // The sum sizes the admission semaphore; tokio's Semaphore panics above MAX_PERMITS, so an
    // oversized config must surface as a clean error rather than a startup crash.
    let mut args = base_args();
    args.max_queued_requests = Some(tokio::sync::Semaphore::MAX_PERMITS);

    let error = ServiceConfig::from_args(args).unwrap_err();

    assert!(matches!(error, ConfigError::InvalidArgument(_)));
}

/// `cors_test.rs` owns the normalization matrix; these cases only pin that `from_args` routes
/// origins through it.
#[rstest]
#[case::disabled(vec![], vec![])]
#[case::normalizes_and_deduplicates(
    vec!["https://example.com", "https://example.com:443/"],
    vec!["https://example.com"],
)]
fn cors_allow_origin_valid_cases(#[case] input: Vec<&str>, #[case] expected: Vec<&str>) {
    let mut args = base_args();
    args.cors_allow_origin = input.into_iter().map(String::from).collect();

    let config = ServiceConfig::from_args(args).unwrap();

    assert_eq!(config.cors_allow_origin, expected);
}

#[test]
fn cors_allow_origin_rejection_propagates() {
    let mut args = base_args();
    args.cors_allow_origin = vec!["http://localhost:5173/path".to_string()];

    let error = ServiceConfig::from_args(args).unwrap_err();

    assert_matches!(error, ConfigError::InvalidArgument(_));
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
        max_queued_requests: None,
        queue_wait_timeout_millis: None,
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
        blocking_check_timeout_millis: None,
        blocking_check_fail_open: None,
        max_request_body_size: None,
        ohttp_enabled: false,
        ohttp_key_cache_max_age_secs: None,
        log_format: LogFormat::Text,
    };

    let error = ServiceConfig::from_args(args).unwrap_err();

    assert!(matches!(error, ConfigError::ConfigFileError(_)));
}

#[test]
fn rejects_zero_queue_wait_timeout_with_a_queue() {
    let mut args = base_args();
    args.max_queued_requests = Some(1);
    args.queue_wait_timeout_millis = Some(0);

    let error = ServiceConfig::from_args(args).unwrap_err();

    assert!(matches!(error, ConfigError::InvalidArgument(_)));
}

#[test]
fn allows_zero_queue_wait_timeout_without_a_queue() {
    // With no queue, a request never waits, so a zero backstop is harmless.
    let mut args = base_args();
    args.max_queued_requests = Some(0);
    args.queue_wait_timeout_millis = Some(0);

    ServiceConfig::from_args(args).unwrap();
}

#[rstest]
#[case::cert_without_key(Some("cert.pem".into()), None, true)]
#[case::key_without_cert(None, Some("key.pem".into()), true)]
#[case::both_provided(Some("cert.pem".into()), Some("key.pem".into()), false)]
#[case::neither_provided(None, None, false)]
fn tls_config_validation(
    #[case] tls_cert_file: Option<PathBuf>,
    #[case] tls_key_file: Option<PathBuf>,
    #[case] expect_incomplete_tls_config: bool,
) {
    let mut args = base_args();
    args.tls_cert_file = tls_cert_file;
    args.tls_key_file = tls_key_file;

    let result = ServiceConfig::from_args(args);

    if expect_incomplete_tls_config {
        assert_matches!(result.unwrap_err(), ConfigError::IncompleteTlsConfig(_));
    } else {
        result.unwrap();
    }
}

/// A config file supplying only `tls_cert_file`, with no CLI key to complete it, is the one
/// partial-TLS shape the CLI-only cases above cannot express.
#[test]
fn config_file_tls_cert_without_key_is_rejected() {
    let mut config_file = NamedTempFile::new().unwrap();
    write!(config_file, r#"{{"rpc_node_url":"http://localhost:9545","tls_cert_file":"cert.pem"}}"#)
        .unwrap();

    let mut args = base_args();
    args.config_file = Some(config_file.path().to_path_buf());
    args.rpc_url = None;

    let error = ServiceConfig::from_args(args).unwrap_err();

    assert_matches!(error, ConfigError::IncompleteTlsConfig(_));
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

#[test]
fn missing_rpc_url_rejected() {
    let mut args = base_args();
    args.rpc_url = None;

    assert_matches!(
        ServiceConfig::from_args(args).unwrap_err(),
        ConfigError::MissingRequiredField(message) if message.contains("rpc_node_url")
    );
}

#[rstest]
#[case::max_concurrent_requests_zero(
    |args: &mut CliArgs| args.max_concurrent_requests = Some(0),
    "max_concurrent_requests"
)]
#[case::max_connections_zero(
    |args: &mut CliArgs| args.max_connections = Some(0),
    "max_connections"
)]
#[case::max_request_body_size_zero(
    |args: &mut CliArgs| args.max_request_body_size = Some(0),
    "max_request_body_size"
)]
fn zero_limits_rejected(#[case] set_zero_limit: fn(&mut CliArgs), #[case] expected_message: &str) {
    let mut args = base_args();
    set_zero_limit(&mut args);

    assert_matches!(
        ServiceConfig::from_args(args).unwrap_err(),
        ConfigError::InvalidArgument(message) if message.contains(expected_message)
    );
}

#[test]
fn max_concurrent_requests_of_one_accepted() {
    let mut args = base_args();
    args.max_concurrent_requests = Some(1);

    let config = ServiceConfig::from_args(args).unwrap();

    assert_eq!(config.max_concurrent_requests, 1);
}

#[rstest]
#[case::invalid_ip(|args: &mut CliArgs| args.ip = Some("not-an-ip".to_string()), "IP address")]
#[case::invalid_strk_fee_token_address(
    |args: &mut CliArgs| args.strk_fee_token_address = Some("not-an-address".to_string()),
    "strk_fee_token_address"
)]
#[case::invalid_blocking_check_url(
    |args: &mut CliArgs| args.blocking_check_url = Some("not-a-url".to_string()),
    "blocking_check_url"
)]
fn invalid_argument_parse_rejected(
    #[case] set_invalid_value: fn(&mut CliArgs),
    #[case] expected_message: &str,
) {
    let mut args = base_args();
    set_invalid_value(&mut args);

    assert_matches!(
        ServiceConfig::from_args(args).unwrap_err(),
        ConfigError::InvalidArgument(message) if message.contains(expected_message)
    );
}

#[test]
fn no_cors_with_cors_allow_origin_rejected() {
    let mut args = base_args();
    args.no_cors = true;
    args.cors_allow_origin = vec!["http://localhost:5173".to_string()];

    assert_matches!(
        ServiceConfig::from_args(args).unwrap_err(),
        ConfigError::InvalidArgument(message) if message.contains("mutually exclusive")
    );
}

#[test]
fn skip_fee_field_validation_disables_validation() {
    let mut args = base_args();
    args.skip_fee_field_validation = true;

    let config = ServiceConfig::from_args(args).unwrap();

    assert!(!config.prover_config.validate_zero_fee_fields);
}

#[test]
fn no_cors_clears_config_file_origins() {
    let mut config_file = NamedTempFile::new().unwrap();
    write!(
        config_file,
        r#"{{"rpc_node_url":"http://localhost:9545","cors_allow_origin":["http://localhost:5173"]}}"#,
    )
    .unwrap();

    let mut args = base_args();
    args.config_file = Some(config_file.path().to_path_buf());
    args.rpc_url = None;
    args.no_cors = true;

    let config = ServiceConfig::from_args(args).unwrap();

    assert!(config.cors_allow_origin.is_empty());
}

#[test]
fn config_file_values_used_when_no_cli_overrides() {
    let mut config_file = NamedTempFile::new().unwrap();
    write!(
        config_file,
        r#"{{"rpc_node_url":"http://localhost:9545","port":8080,"ip":"127.0.0.1"}}"#,
    )
    .unwrap();

    let mut args = base_args();
    args.config_file = Some(config_file.path().to_path_buf());
    args.rpc_url = None;

    let config = ServiceConfig::from_args(args).unwrap();

    assert_eq!(config.port, 8080);
    assert_eq!(config.ip, "127.0.0.1".parse::<IpAddr>().unwrap());
}

#[test]
fn cli_overrides_config_file_values() {
    let mut config_file = NamedTempFile::new().unwrap();
    write!(config_file, r#"{{"rpc_node_url":"http://localhost:9545","port":8080}}"#).unwrap();

    let mut args = base_args();
    args.config_file = Some(config_file.path().to_path_buf());
    args.rpc_url = None;
    args.port = Some(9090);

    let config = ServiceConfig::from_args(args).unwrap();

    assert_eq!(config.port, 9090);
}
