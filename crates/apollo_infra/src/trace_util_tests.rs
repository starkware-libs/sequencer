use std::io::Write;
use std::sync::{Arc, Mutex};

use thiserror::Error;
use tracing::instrument;
use tracing::metadata::LevelFilter;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{EnvFilter, reload};

use crate::trace_util::{
    ReloadHandle, create_fmt_layer, get_log_directives, rename_error_to_message, set_log_level,
};

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
fn rename_error_to_message_preserves_existing_message_field() {
    // If both "error" and "message" exist, leave the object unchanged
    let input = br#"{"error":"the error","message":"original message"}"#;
    let output = rename_error_to_message(input).unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&output).unwrap();

    // Both fields should remain unchanged
    assert_eq!(parsed["message"], "original message");
    assert_eq!(parsed["error"], "the error");
}

#[test]
fn rename_error_to_message_does_not_modify_error_values() {
    // Values equal to "error" should NOT be modified - only keys named "error"
    let input = br#"{"status":"error","type":"error","level":"ERROR"}"#;
    let output = rename_error_to_message(input).unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&output).unwrap();

    assert_eq!(parsed["status"], "error");
    assert_eq!(parsed["type"], "error");
    assert_eq!(parsed["level"], "ERROR");
}

/// A shared buffer for capturing log output.
#[derive(Clone)]
struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

impl Write for SharedBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

const TEST_ERROR_MESSAGE: &str = "something went wrong";

#[derive(Debug, Error)]
#[error("{0}")]
struct TestError(&'static str);

#[instrument(err)]
fn failing_function() -> Result<(), TestError> {
    Err(TestError(TEST_ERROR_MESSAGE))
}

/// Tests that create_fmt_layer (used by configure_tracing) renames "error" to "message".
/// This verifies that #[instrument(err)] errors are logged with "message" instead of "error".
#[test]
fn create_fmt_layer_renames_error_to_message() {
    let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
    let buffer_clone = buffer.clone();

    // Use the same create_fmt_layer as configure_tracing(), with a capturing writer.
    let subscriber =
        tracing_subscriber::registry().with(create_fmt_layer(move || buffer_clone.clone()));

    tracing::subscriber::with_default(subscriber, || {
        let _ = failing_function();
    });

    let output = String::from_utf8(buffer.0.lock().unwrap().clone()).unwrap();

    // The output should contain "message" instead of "error" for the error value.
    let expected_message = format!(r#""message":"{TEST_ERROR_MESSAGE}""#);
    assert!(
        output.contains(&expected_message),
        "Expected 'message' key with error value, got: {output}"
    );

    // The raw "error" key with the error value should NOT be present.
    let unexpected_error = format!(r#""error":"{TEST_ERROR_MESSAGE}""#);
    assert!(
        !output.contains(&unexpected_error),
        "Did not expect 'error' key with error value, got: {output}"
    );
}
