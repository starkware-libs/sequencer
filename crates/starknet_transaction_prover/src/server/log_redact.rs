//! Helpers for sanitizing log lines and the values that appear in them.

use std::ffi::OsString;

#[cfg(test)]
#[path = "log_redact_test.rs"]
mod log_redact_test;

/// Returns `scheme://host[:port]` for a URL, dropping userinfo, path, query,
/// and fragment. Used to log upstream endpoints without echoing credentials
/// embedded in the URL. Falls back to `"<invalid url>"` on parse failure so
/// the raw URL is never echoed.
pub fn redact_url_host(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(parsed) => {
            let host = parsed.host_str().unwrap_or("");
            match parsed.port() {
                Some(port) => format!("{}://{}:{}", parsed.scheme(), host, port),
                None => format!("{}://{}", parsed.scheme(), host),
            }
        }
        Err(_) => "<invalid url>".to_string(),
    }
}

/// Returns whether the text log format emits ANSI styling: only when stdout is a terminal and
/// `no_color`, the value of the `NO_COLOR` environment variable (<https://no-color.org>), is unset
/// or empty.
///
/// A container scheduler pipes stdout to a log aggregator, which would receive the escape
/// sequences verbatim. `fmt::Layer::with_ansi` overrides tracing-subscriber's own `NO_COLOR`
/// check, so the `NO_COLOR` rule has to be applied here.
pub fn text_log_ansi_enabled(stdout_is_terminal: bool, no_color: Option<OsString>) -> bool {
    stdout_is_terminal && no_color.is_none_or(|value| value.is_empty())
}
