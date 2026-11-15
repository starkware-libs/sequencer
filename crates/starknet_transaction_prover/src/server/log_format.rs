//! The JSON log format, shaped for Google Cloud Logging.
//!
//! Cloud Logging reads a container's JSON log line into `jsonPayload` and takes the entry's
//! severity from a top-level `severity` field and its summary line from a top-level `message`
//! field (<https://cloud.google.com/logging/docs/structured-logging>). `tracing-subscriber`'s JSON
//! formatter names the level `level` and nests the message and event fields under `fields`, so
//! this layer flattens the event fields to the top level and adds `severity` next to `level`.

use std::io::{Result as IoResult, Write};

use serde_json::{Map, Value};
use tracing::Subscriber;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{fmt, Layer};

#[cfg(test)]
#[path = "log_format_test.rs"]
mod log_format_test;

/// Returns the JSON log layer: one object per line, with the event's fields (including
/// `message`) at the top level and a Cloud Logging `severity` derived from `level`.
pub fn json_log_layer<S, F, W>(make_writer: F) -> impl Layer<S>
where
    S: Subscriber + for<'span> LookupSpan<'span>,
    F: Fn() -> W + Send + Sync + 'static,
    W: Write + 'static,
{
    fmt::layer()
        .json()
        .flatten_event(true)
        .with_writer(make_writer)
        .map_writer(|make_writer| move || SeverityWriter(make_writer()))
}

/// Maps a `tracing` level, as the JSON formatter writes it, to a Cloud Logging `LogSeverity`.
/// Cloud Logging has no `TRACE`, so it maps to `DEBUG`.
fn cloud_logging_severity(level: &str) -> Option<&'static str> {
    match level {
        "TRACE" | "DEBUG" => Some("DEBUG"),
        "INFO" => Some("INFO"),
        "WARN" => Some("WARNING"),
        "ERROR" => Some("ERROR"),
        _ => None,
    }
}

/// Returns the log line with a `severity` field added as its first key, or `None` to write the
/// line unchanged: when it is not a JSON object, has no recognized `level`, or already carries a
/// `severity`. The rest of the line is kept byte for byte, so the formatter's key order survives.
fn with_severity(log_line: &[u8]) -> Option<Vec<u8>> {
    let log_object: Map<String, Value> = serde_json::from_slice(log_line).ok()?;
    if log_object.contains_key("severity") {
        return None;
    }
    let severity = cloud_logging_severity(log_object.get("level")?.as_str()?)?;
    // A JSON object with a `level` key is non-empty, so a comma separates `severity` from it.
    let fields_after_open_brace = log_line.trim_ascii_start().strip_prefix(b"{")?;
    let severity_prefix = format!("{{\"severity\":\"{severity}\",");
    Some([severity_prefix.as_bytes(), fields_after_open_brace].concat())
}

/// Adds `severity` to each JSON log line before passing it to the wrapped writer. The JSON
/// formatter writes one whole line per `write_all` call, so each `write` sees one complete object.
struct SeverityWriter<W>(W);

impl<W: Write> Write for SeverityWriter<W> {
    fn write(&mut self, log_line: &[u8]) -> IoResult<usize> {
        match with_severity(log_line) {
            Some(rewritten_line) => self.0.write_all(&rewritten_line)?,
            None => self.0.write_all(log_line)?,
        }
        Ok(log_line.len())
    }

    fn flush(&mut self) -> IoResult<()> {
        self.0.flush()
    }
}
