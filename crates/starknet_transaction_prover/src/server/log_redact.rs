//! Helpers for sanitizing values that appear in log lines.

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
