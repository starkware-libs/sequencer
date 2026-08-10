use std::io::Write;
use std::sync::{Arc, Mutex};

use thiserror::Error;
use tracing::instrument;
use tracing::metadata::LevelFilter;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{reload, EnvFilter};

use crate::trace_util::{
    create_fmt_layer,
    get_log_directives,
    rename_error_to_message,
    set_log_level,
    ErrorToMessageWriter,
    ReloadHandle,
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
fn rename_error_to_message_returns_none_when_no_error_key_present() {
    // No "error" key at all: no rewrite needed, caller writes the buffer unchanged.
    let input = br#"{"level":"INFO","status":"ok","count":42}"#;
    assert!(rename_error_to_message(input).is_none());
}

#[test]
fn rename_error_to_message_returns_none_for_invalid_json() {
    // Contains the "error" byte pattern so it reaches the JSON parse, which then fails.
    let input = br#"not valid json "error""#;
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
    // If both "error" and "message" exist, no rewrite is needed: the caller writes the buffer
    // unchanged, which trivially preserves both fields.
    let input = br#"{"error":"the error","message":"original message"}"#;
    assert!(rename_error_to_message(input).is_none());
}

#[test]
fn rename_error_to_message_returns_none_when_error_appears_only_as_a_value() {
    // Values equal to "error" should NOT be modified - only keys named "error". There is no
    // actual "error" key here, so no rewrite is needed even though the prefilter matches.
    let input = br#"{"status":"error","type":"error","level":"ERROR"}"#;
    assert!(rename_error_to_message(input).is_none());
}

#[test]
fn error_to_message_writer_passes_through_non_error_lines_unchanged() {
    let mut buffer = Vec::new();
    let mut writer = ErrorToMessageWriter(&mut buffer);

    let plain_line: &[u8] = br#"{"level":"INFO","message":"hello"}"#;
    let plain_line = [plain_line, b"\n"].concat();
    let error_line: &[u8] = br#"{"level":"ERROR","error":"boom"}"#;
    let error_line = [error_line, b"\n"].concat();

    writer.write_all(&plain_line).unwrap();
    writer.write_all(&error_line).unwrap();

    let output = String::from_utf8(buffer).unwrap();
    let mut lines = output.lines();

    // The plain line has no "error" key, so it must come out byte-for-byte unchanged.
    assert_eq!(lines.next().unwrap().as_bytes(), &plain_line[..plain_line.len() - 1]);

    // The error line is still rewritten, and the two lines stay newline-separated.
    let rewritten: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
    assert_eq!(rewritten["message"], "boom");
    assert!(rewritten.get("error").is_none());
    assert!(lines.next().is_none());
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

/// The JSON layer must not emit the `span` field. `spans` already carries the full span list with
/// the current span as its last element, so `span` duplicates it on every single log entry.
#[test]
fn create_fmt_layer_omits_the_redundant_current_span_field() {
    let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
    let buffer_clone = buffer.clone();

    let subscriber =
        tracing_subscriber::registry().with(create_fmt_layer(move || buffer_clone.clone()));

    tracing::subscriber::with_default(subscriber, || {
        let span = tracing::info_span!("run_height", height = 7);
        let _enter = span.enter();
        tracing::info!("inside the span");
    });

    let output = String::from_utf8(buffer.0.lock().unwrap().clone()).unwrap();
    let entry: serde_json::Value = serde_json::from_str(output.trim()).unwrap();

    assert!(entry.get("span").is_none(), "the duplicated `span` field should be absent: {output}");

    // `spans` must still be there, and must still expose the current span's fields — that is what
    // makes `span` redundant rather than merely expensive.
    let spans = entry["spans"].as_array().expect("`spans` should still be emitted: {output}");
    let current_span = spans.last().expect("`spans` should contain the current span");
    assert_eq!(current_span["name"], "run_height");
    assert_eq!(current_span["height"], 7);
}
