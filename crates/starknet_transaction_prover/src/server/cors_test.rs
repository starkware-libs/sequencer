use assert_matches::assert_matches;
use rstest::rstest;

use crate::errors::ConfigError;
use crate::server::cors::{build_cors_layer, cors_mode, normalize_cors_allow_origins};

#[test]
fn test_build_cors_layer_returns_none_for_empty_origins() {
    let result = build_cors_layer(&[]).expect("build_cors_layer should not fail");
    assert!(result.is_none(), "Expected None for empty origins list");
}

#[test]
fn test_build_cors_layer_returns_some_for_wildcard() {
    let origins = vec!["*".to_string()];
    let result = build_cors_layer(&origins).expect("build_cors_layer should not fail");
    assert!(result.is_some(), "Expected Some for wildcard origin");
}

#[test]
fn test_build_cors_layer_returns_some_for_allowlist() {
    let origins = vec!["http://example.com".to_string()];
    let result = build_cors_layer(&origins).expect("build_cors_layer should not fail");
    assert!(result.is_some(), "Expected Some for non-empty allowlist");
}

#[rstest]
#[case::disabled(vec![], "disabled")]
#[case::wildcard(vec!["*".to_string()], "wildcard")]
#[case::allowlist(vec!["http://example.com".to_string()], "allowlist")]
#[case::multiple_origins(vec!["http://a.com".to_string(), "http://b.com".to_string()], "allowlist")]
fn test_cors_mode_labels(#[case] origins: Vec<String>, #[case] expected_label: &str) {
    assert_eq!(cors_mode(&origins), expected_label);
}

#[test]
fn test_normalize_rejects_ftp_scheme() {
    let result = normalize_cors_allow_origins(vec!["ftp://example.com".to_string()]);
    assert_matches!(result, Err(ConfigError::InvalidArgument(_)));
}

#[test]
fn test_normalize_rejects_missing_host() {
    let result = normalize_cors_allow_origins(vec!["http://".to_string()]);
    assert_matches!(result, Err(ConfigError::InvalidArgument(_)));
}

#[test]
fn test_normalize_rejects_userinfo() {
    let result = normalize_cors_allow_origins(vec!["http://user:pass@example.com".to_string()]);
    assert_matches!(result, Err(ConfigError::InvalidArgument(_)));
}

#[test]
fn test_normalize_strips_default_http_port() {
    let result = normalize_cors_allow_origins(vec!["http://example.com:80".to_string()])
        .expect("normalize should succeed");
    assert_eq!(result, vec!["http://example.com".to_string()]);
}

#[test]
fn test_normalize_strips_default_https_port() {
    let result = normalize_cors_allow_origins(vec!["https://example.com:443".to_string()])
        .expect("normalize should succeed");
    assert_eq!(result, vec!["https://example.com".to_string()]);
}

#[test]
fn test_normalize_preserves_non_default_port() {
    let result = normalize_cors_allow_origins(vec!["http://example.com:8080".to_string()])
        .expect("normalize should succeed");
    assert_eq!(result, vec!["http://example.com:8080".to_string()]);
}
