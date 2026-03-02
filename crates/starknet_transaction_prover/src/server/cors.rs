//! CORS configuration utilities for the JSON-RPC server.

use anyhow::Context;
use http::{header, HeaderValue, Method};
use tower_http::cors::{AllowOrigin, CorsLayer};
use url::Url;

use crate::errors::ConfigError;

/// Builds a tower-http `CorsLayer` from the (already normalized) list of allowed origins.
///
/// Returns `None` when CORS is disabled (empty list), or `Some(layer)` for wildcard / allowlist
/// mode.
pub fn build_cors_layer(
    normalized_cors_allow_origin: &[String],
) -> anyhow::Result<Option<CorsLayer>> {
    if normalized_cors_allow_origin.is_empty() {
        return Ok(None);
    }

    // Origins are already validated/normalized by `ServiceConfig::from_args`, so
    // `HeaderValue::from_str` should never fail here.
    let allow_origin = if normalized_cors_allow_origin == ["*"] {
        AllowOrigin::any()
    } else {
        let header_values = normalized_cors_allow_origin
            .iter()
            .map(|origin| {
                HeaderValue::from_str(origin)
                    .context(format!("Invalid cors_allow_origin header value '{origin}'"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        AllowOrigin::list(header_values)
    };

    Ok(Some(
        CorsLayer::new()
            .allow_origin(allow_origin)
            .allow_methods([Method::POST])
            .allow_headers([header::CONTENT_TYPE]),
    ))
}

/// Returns a human-readable label for the active CORS mode.
pub fn cors_mode(normalized_cors_allow_origin: &[String]) -> &'static str {
    match normalized_cors_allow_origin {
        [] => "disabled",
        [single] if single == "*" => "wildcard",
        _ => "allowlist",
    }
}

/// Normalizes a list of CORS origins: if any entry is `"*"` the result is `["*"]`, otherwise each
/// origin is parsed/validated and deduplicated.
pub(crate) fn normalize_cors_allow_origins(
    cors_allow_origins: Vec<String>,
) -> Result<Vec<String>, ConfigError> {
    // If any entry is "*", treat it as "allow all origins" and ignore any other values.
    if cors_allow_origins.iter().any(|origin| origin == "*") {
        return Ok(vec!["*".to_string()]);
    }

    let mut normalized_allow_origins = Vec::new();

    for cors_allow_origin in cors_allow_origins {
        let normalized_allow_origin = normalize_cors_allow_origin(&cors_allow_origin)?;
        if !normalized_allow_origins.contains(&normalized_allow_origin) {
            normalized_allow_origins.push(normalized_allow_origin);
        }
    }

    Ok(normalized_allow_origins)
}

/// Parses and normalizes a single CORS origin string.
///
/// Validates that the origin uses http/https, has a host, contains no
/// path/query/fragment/userinfo, and strips default ports (80 for http, 443 for https).
fn normalize_cors_allow_origin(cors_allow_origin: &str) -> Result<String, ConfigError> {
    let parsed = Url::parse(cors_allow_origin).map_err(|e| {
        ConfigError::InvalidArgument(format!(
            "Invalid cors_allow_origin '{cors_allow_origin}': {e}"
        ))
    })?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(ConfigError::InvalidArgument(format!(
            "Invalid cors_allow_origin '{cors_allow_origin}': only http:// and https:// are \
             supported."
        )));
    }
    if parsed.host().is_none() {
        return Err(ConfigError::InvalidArgument(format!(
            "Invalid cors_allow_origin '{cors_allow_origin}': host is required."
        )));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(ConfigError::InvalidArgument(format!(
            "Invalid cors_allow_origin '{cors_allow_origin}': userinfo is not supported."
        )));
    }
    if parsed.path() != "/" || parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(ConfigError::InvalidArgument(format!(
            "Invalid cors_allow_origin '{cors_allow_origin}': must be '*' or \
             '<scheme>://<host>[:port]' without a path, query, or fragment."
        )));
    }

    let host = parsed.host().expect("host presence validated above");
    let mut normalized = format!("{}://{}", parsed.scheme(), host);

    if let Some(port) = parsed.port() {
        let is_default_port = (parsed.scheme() == "http" && port == 80)
            || (parsed.scheme() == "https" && port == 443);
        if !is_default_port {
            normalized.push(':');
            normalized.push_str(&port.to_string());
        }
    }

    Ok(normalized)
}
