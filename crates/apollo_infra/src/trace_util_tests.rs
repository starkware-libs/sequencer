use tracing::metadata::LevelFilter;
use tracing_subscriber::{reload, EnvFilter};

use crate::trace_util::{get_log_directives, rename_error_to_message, set_log_level, ReloadHandle};

#[test]
fn log_level_directive_updates() {
    let filter = EnvFilter::new("info");
    let (_layer, reload_handle): (reload::Layer<_, _>, ReloadHandle) = reload::Layer::new(filter);

    set_log_level(&reload_handle, "a", LevelFilter::DEBUG);
    set_log_level(&reload_handle, "b", LevelFilter::DEBUG);
    let directives = get_log_directives(&reload_handle).unwrap();
    assert_eq!(directives, "b=debug,a=debug,info");
    set_log_level(&reload_handle, "a", LevelFilter::INFO);
    let directives = get_log_directives(&reload_handle).unwrap();
    assert_eq!(directives, "b=debug,a=info,info");
}

#[test]
fn rename_error_to_message_renames_error_key() {
    let input = br#"{"level":"ERROR","error":"something failed","file":"test.rs"}"#;
    let output = rename_error_to_message(input).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    assert!(output_str.contains(r#""message":"something failed""#), "got: {output_str}");
    assert!(!output_str.contains(r#""error""#), "got: {output_str}");
}

#[test]
fn rename_error_to_message_preserves_other_fields() {
    let input = br#"{"level":"INFO","status":"ok","count":42}"#;
    let output = rename_error_to_message(input).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    assert!(output_str.contains(r#""level":"INFO""#), "got: {output_str}");
    assert!(output_str.contains(r#""status":"ok""#), "got: {output_str}");
    assert!(output_str.contains(r#""count":42"#), "got: {output_str}");
}

#[test]
fn rename_error_to_message_returns_none_for_invalid_json() {
    let input = b"not valid json";
    assert!(rename_error_to_message(input).is_none());
}

#[test]
fn rename_error_to_message_only_renames_root_level_error() {
    // Nested "error" fields should NOT be renamed - only root level
    let input = br#"{"error":"root error","nested":{"error":"nested error"}}"#;
    let output = rename_error_to_message(input).unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&output).unwrap();

    // Root "error" should be renamed to "message"
    assert_eq!(parsed["message"], "root error");
    assert!(parsed.get("error").is_none(), "root 'error' should be removed");

    // Nested "error" should remain unchanged
    assert_eq!(parsed["nested"]["error"], "nested error");
}

#[test]
fn rename_error_to_message_overwrites_existing_message_field() {
    // If both "error" and "message" exist, "error" value overwrites "message"
    let input = br#"{"error":"the error","message":"original message"}"#;
    let output = rename_error_to_message(input).unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&output).unwrap();

    // "error" value should overwrite "message"
    assert_eq!(parsed["message"], "the error");
    assert!(parsed.get("error").is_none());
}

#[test]
fn rename_error_to_message_with_duplicate_error_keys_uses_last_value() {
    // Invalid JSON with duplicate keys - serde_json keeps the LAST value
    let input = br#"{"error":"first","error":"second"}"#;
    let output = rename_error_to_message(input).unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&output).unwrap();

    // serde_json keeps the last "error" value, which then gets renamed to "message"
    assert_eq!(parsed["message"], "second");
    assert!(parsed.get("error").is_none());
}
