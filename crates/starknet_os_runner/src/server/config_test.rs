use std::io::Write;

use clap::Parser;
use tempfile::NamedTempFile;

use crate::server::config::{CliArgs, ConfigError, ServiceConfig};

fn base_args() -> CliArgs {
    CliArgs {
        config_file: None,
        rpc_url: Some("http://localhost:9545".to_string()),
        chain_id: None,
        port: None,
        ip: None,
        max_concurrent_requests: None,
        max_connections: None,
        cors_allow_origin: Vec::new(),
    }
}

#[test]
fn from_args_accepts_wildcard_cors_allow_origin() {
    let mut args = base_args();
    args.cors_allow_origin = vec!["*".to_string()];

    let config = ServiceConfig::from_args(args).unwrap();

    assert_eq!(config.cors_allow_origin, vec!["*"]);
}

#[test]
fn from_args_accepts_multiple_cors_allow_origins() {
    let mut args = base_args();
    args.cors_allow_origin =
        vec!["https://example.com".to_string(), "http://localhost:5173".to_string()];

    let config = ServiceConfig::from_args(args).unwrap();

    assert_eq!(
        config.cors_allow_origin,
        vec!["https://example.com".to_string(), "http://localhost:5173".to_string()]
    );
}

#[test]
fn from_args_normalizes_cors_allow_origin() {
    let mut args = base_args();
    args.cors_allow_origin = vec!["https://example.com:443/".to_string()];

    let config = ServiceConfig::from_args(args).unwrap();

    assert_eq!(config.cors_allow_origin, vec!["https://example.com".to_string()]);
}

#[test]
fn from_args_rejects_invalid_cors_allow_origin() {
    let mut args = base_args();
    args.cors_allow_origin = vec!["http://localhost:5173/path".to_string()];

    let error = ServiceConfig::from_args(args).unwrap_err();

    assert!(matches!(error, ConfigError::InvalidArgument(_)));
}

#[test]
fn from_args_deduplicates_cors_allow_origins() {
    let mut args = base_args();
    args.cors_allow_origin = vec![
        "https://example.com".to_string(),
        "https://example.com:443/".to_string(),
        "http://localhost:5173".to_string(),
        "http://localhost:5173".to_string(),
    ];

    let config = ServiceConfig::from_args(args).unwrap();

    assert_eq!(
        config.cors_allow_origin,
        vec!["https://example.com".to_string(), "http://localhost:5173".to_string()]
    );
}

#[test]
fn from_args_prioritizes_wildcard_over_explicit_origins() {
    let mut args = base_args();
    args.cors_allow_origin = vec![
        "https://example.com".to_string(),
        "*".to_string(),
        "http://localhost:5173".to_string(),
    ];

    let config = ServiceConfig::from_args(args).unwrap();

    assert_eq!(config.cors_allow_origin, vec!["*".to_string()]);
}

#[test]
fn from_args_wildcard_ignores_invalid_explicit_origins() {
    let mut args = base_args();
    args.cors_allow_origin = vec!["*".to_string(), "://invalid-origin".to_string()];

    let config = ServiceConfig::from_args(args).unwrap();

    assert_eq!(config.cors_allow_origin, vec!["*".to_string()]);
}

#[test]
fn cli_parses_repeated_cors_allow_origin_flags() {
    let args = CliArgs::parse_from([
        "starknet-os-runner",
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
fn from_args_rejects_legacy_string_cors_allow_origin_in_config_file() {
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
        cors_allow_origin: Vec::new(),
    };

    let error = ServiceConfig::from_args(args).unwrap_err();

    assert!(matches!(error, ConfigError::ConfigFileError(_)));
}
