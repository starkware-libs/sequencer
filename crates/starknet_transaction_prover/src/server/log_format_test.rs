use std::io::Write;
use std::sync::{Arc, Mutex};

use rstest::rstest;
use serde_json::Value;
use tracing::Level;
use tracing_subscriber::prelude::*;

use crate::server::log_format::{json_log_layer, SeverityWriter};

/// Collects everything written to it, so a test can read back the log lines.
#[derive(Clone, Default)]
struct CapturedOutput(Arc<Mutex<Vec<u8>>>);

impl CapturedOutput {
    fn log_lines(&self) -> Vec<Value> {
        let output = self.0.lock().unwrap();
        std::str::from_utf8(&output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
}

impl Write for CapturedOutput {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Emits one event at `level` through the JSON log layer and returns the line it wrote.
fn log_one_event(level: Level) -> Value {
    let captured_output = CapturedOutput::default();
    let layer_output = captured_output.clone();
    let subscriber =
        tracing_subscriber::registry().with(json_log_layer(move || layer_output.clone()));
    tracing::subscriber::with_default(subscriber, || match level {
        Level::TRACE => tracing::trace!(event = "stage_done", duration_ms = 7, "Stage done"),
        Level::DEBUG => tracing::debug!(event = "stage_done", duration_ms = 7, "Stage done"),
        Level::INFO => tracing::info!(event = "stage_done", duration_ms = 7, "Stage done"),
        Level::WARN => tracing::warn!(event = "stage_done", duration_ms = 7, "Stage done"),
        Level::ERROR => tracing::error!(event = "stage_done", duration_ms = 7, "Stage done"),
    });
    let mut log_lines = captured_output.log_lines();
    assert_eq!(log_lines.len(), 1, "expected exactly one log line, got {log_lines:?}");
    log_lines.remove(0)
}

#[rstest]
#[case::trace(Level::TRACE, "TRACE", "DEBUG")]
#[case::debug(Level::DEBUG, "DEBUG", "DEBUG")]
#[case::info(Level::INFO, "INFO", "INFO")]
#[case::warn(Level::WARN, "WARN", "WARNING")]
#[case::error(Level::ERROR, "ERROR", "ERROR")]
fn adds_cloud_logging_severity_next_to_level(
    #[case] level: Level,
    #[case] expected_level: &str,
    #[case] expected_severity: &str,
) {
    let log_line = log_one_event(level);

    assert_eq!(log_line["level"], expected_level);
    assert_eq!(log_line["severity"], expected_severity);
}

#[test]
fn puts_message_and_event_fields_at_top_level() {
    let log_line = log_one_event(Level::INFO);

    assert_eq!(log_line["message"], "Stage done");
    assert_eq!(log_line["event"], "stage_done");
    assert_eq!(log_line["duration_ms"], 7);
    assert!(log_line.get("fields").is_none(), "event fields still nested: {log_line}");
}

#[rstest]
#[case::not_json(b"plain text line\n".as_slice())]
#[case::no_level(b"{\"message\":\"no level\"}\n".as_slice())]
#[case::unknown_level(b"{\"level\":\"NOTICE\"}\n".as_slice())]
#[case::existing_severity(b"{\"level\":\"INFO\",\"severity\":\"NOTICE\"}\n".as_slice())]
fn writes_lines_it_cannot_map_unchanged(#[case] log_line: &[u8]) {
    let captured_output = CapturedOutput::default();
    let mut severity_writer = SeverityWriter(captured_output.clone());

    let bytes_written = severity_writer.write(log_line).unwrap();

    assert_eq!(bytes_written, log_line.len());
    assert_eq!(*captured_output.0.lock().unwrap(), log_line);
}

#[test]
fn prepends_severity_and_keeps_the_rest_of_the_line_and_its_length() {
    let captured_output = CapturedOutput::default();
    let mut severity_writer = SeverityWriter(captured_output.clone());
    let log_line = b"{\"level\":\"WARN\",\"message\":\"slow\"}\n";

    let bytes_written = severity_writer.write(log_line).unwrap();

    assert_eq!(bytes_written, log_line.len());
    assert_eq!(
        *captured_output.0.lock().unwrap(),
        b"{\"severity\":\"WARNING\",\"level\":\"WARN\",\"message\":\"slow\"}\n"
    );
}
