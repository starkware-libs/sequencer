use assert_matches::assert_matches;
use rstest::rstest;

use crate::errors::ConfigError;
use crate::server::cors::{build_cors_layer, cors_mode, normalize_cors_allow_origins};

#[rstest]
#[case::empty(&[], false)]
#[case::wildcard(&["*"], true)]
#[case::allowlist(&["http://example.com"], true)]
fn test_build_cors_layer(#[case] origins: &[&str], #[case] expect_layer: bool) {
    let origins: Vec<String> = origins.iter().map(|s| s.to_string()).collect();
    let layer = build_cors_layer(&origins).unwrap();
    assert_eq!(layer.is_some(), expect_layer);
}

#[rstest]
#[case::disabled(&[], "disabled")]
#[case::wildcard(&["*"], "wildcard")]
#[case::allowlist(&["http://example.com"], "allowlist")]
#[case::multiple_origins(&["http://a.com", "http://b.com"], "allowlist")]
fn test_cors_mode_labels(#[case] origins: &[&str], #[case] expected_label: &str) {
    let origins: Vec<String> = origins.iter().map(|s| s.to_string()).collect();
    assert_eq!(cors_mode(&origins), expected_label);
}

#[rstest]
#[case::ftp_scheme(&["ftp://example.com"])]
#[case::missing_host(&["http://"])]
#[case::userinfo(&["http://user:pass@example.com"])]
#[case::path(&["http://example.com/path"])]
#[case::query(&["http://example.com?q=1"])]
fn test_normalize_rejects_invalid_origin(#[case] origins: &[&str]) {
    let origins: Vec<String> = origins.iter().map(|s| s.to_string()).collect();
    assert_matches!(normalize_cors_allow_origins(origins), Err(ConfigError::InvalidArgument(_)));
}

#[rstest]
#[case::strip_http_default_port(&["http://example.com:80"], &["http://example.com"])]
#[case::strip_https_default_port(&["https://example.com:443"], &["https://example.com"])]
#[case::preserve_non_default_port(&["http://example.com:8080"], &["http://example.com:8080"])]
#[case::dedup_equivalent_origins(
    &["http://example.com", "http://example.com:80"],
    &["http://example.com"],
)]
#[case::wildcard_collapses_others(
    &["http://example.com", "*", "https://foo.bar"],
    &["*"],
)]
fn test_normalize_valid_origin(#[case] input: &[&str], #[case] expected: &[&str]) {
    let input: Vec<String> = input.iter().map(|s| s.to_string()).collect();
    let expected: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
    assert_eq!(normalize_cors_allow_origins(input).unwrap(), expected);
}
