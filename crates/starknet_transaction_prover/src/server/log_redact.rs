//! Helpers for sanitizing values that appear in log lines.
//!
//! Currently exposes [`redact_url_host`], which collapses a URL down to
//! `scheme://host[:port]`. Used to log upstream endpoints (`rpc_node_url`,
//! `blocking_check_url`) without echoing userinfo or query strings — both of
//! which routinely carry credentials in production configurations.

#[cfg(test)]
#[path = "log_redact_test.rs"]
mod log_redact_test;

/// Returns `scheme://host[:port]` for a URL, dropping any userinfo, path,
/// query, and fragment. Falls back to `"<invalid url>"` when the input cannot
/// be parsed — the raw URL is never echoed even on the error path.
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
