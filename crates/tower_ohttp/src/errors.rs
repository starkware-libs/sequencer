//! `OhttpError` — unified error type for gateway initialization and
//! per-request processing.

use bytes::Bytes;
use http::header;
use http_body_util::Full;

/// Errors produced by OHTTP gateway setup and request/response processing.
#[derive(Debug, thiserror::Error)]
pub enum OhttpError {
    #[error("OHTTP_KEY environment variable not set")]
    MissingKeyEnvVar,

    /// Parsing or length validation of a raw key failed. String payload is a
    /// diagnostic intended for logs, not for clients.
    #[error("invalid OHTTP key: {0}")]
    InvalidKey(String),

    /// The underlying `ohttp` crate rejected the key config (e.g. unsupported
    /// KEM / suite combination, or internal HPKE setup failure).
    #[error("failed to build OHTTP key config: {0}")]
    KeyConfig(#[source] ohttp::Error),

    #[error("OHTTP decapsulation failed")]
    DecapsulationFailed,

    /// The decrypted payload was not a valid Binary HTTP message, or the
    /// rebuilt inner HTTP request could not be constructed from it.
    #[error("invalid Binary HTTP message: {0}")]
    InvalidFormat(&'static str),

    #[error("request body exceeds size limit")]
    BodyTooLarge,

    #[error("failed to read request body")]
    BadRequestBody,

    /// Catch-all for internal errors during response encapsulation — things
    /// that indicate a bug in the layer itself rather than bad input.
    #[error("internal error: {0}")]
    Internal(&'static str),
}

impl OhttpError {
    /// HTTP status code this error maps to.
    pub fn status(&self) -> http::StatusCode {
        match self {
            Self::DecapsulationFailed | Self::InvalidFormat(_) => {
                http::StatusCode::UNPROCESSABLE_ENTITY
            }
            Self::BodyTooLarge => http::StatusCode::PAYLOAD_TOO_LARGE,
            Self::BadRequestBody => http::StatusCode::BAD_REQUEST,
            // Initialization errors and internal errors become 500. In practice
            // initialization errors never reach this path — they're handled at
            // startup and propagated via `?`.
            Self::MissingKeyEnvVar
            | Self::InvalidKey(_)
            | Self::KeyConfig(_)
            | Self::Internal(_) => http::StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Machine-readable error code included in the JSON error body.
    pub fn code(&self) -> &'static str {
        match self {
            Self::DecapsulationFailed => "OHTTP_DECAPSULATION_FAILED",
            Self::InvalidFormat(_) => "OHTTP_INVALID_FORMAT",
            Self::BodyTooLarge => "OHTTP_BODY_TOO_LARGE",
            Self::BadRequestBody => "OHTTP_INVALID_FORMAT",
            Self::MissingKeyEnvVar
            | Self::InvalidKey(_)
            | Self::KeyConfig(_)
            | Self::Internal(_) => "INTERNAL_ERROR",
        }
    }

    /// Human-readable error message for the JSON error body.
    pub fn message(&self) -> &str {
        match self {
            Self::DecapsulationFailed => "Failed to decapsulate OHTTP request",
            Self::InvalidFormat(detail) => detail,
            Self::BodyTooLarge => "Request body exceeds the size limit",
            Self::BadRequestBody => "Failed to read request body",
            Self::MissingKeyEnvVar => "OHTTP gateway not initialized",
            Self::InvalidKey(_) => "invalid OHTTP key",
            Self::KeyConfig(_) => "failed to build OHTTP key config",
            Self::Internal(detail) => detail,
        }
    }

    /// Convert into a plaintext JSON HTTP response with `Full<Bytes>` body.
    /// The body is always `application/json` of the shape
    /// `{"error": {"code": "...", "message": "..."}}`.
    pub fn into_response(self) -> http::Response<Full<Bytes>> {
        let body = serde_json::json!({
            "error": { "code": self.code(), "message": self.message() }
        });
        http::Response::builder()
            .status(self.status())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Full::new(Bytes::from(body.to_string())))
            .expect("error response builder should not fail")
    }
}
