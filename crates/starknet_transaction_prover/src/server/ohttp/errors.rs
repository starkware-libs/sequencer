//! OHTTP processing error types and their HTTP response mappings.

use http::header;
use jsonrpsee::server::{HttpBody, HttpResponse};

/// Errors during OHTTP request/response processing.
pub(crate) enum OhttpProcessingError {
    /// 422 — OHTTP decapsulation failed (bad HPKE envelope).
    DecapsulationFailed,
    /// 422 — invalid Binary HTTP format or request rebuild failure.
    InvalidFormat(&'static str),
    /// 413 — request body exceeds the size limit.
    BodyTooLarge,
    /// 400 — failed to read request body.
    BadRequestBody,
    /// 500 — internal error during response encapsulation.
    InternalError(&'static str),
}

impl From<OhttpProcessingError> for HttpResponse<HttpBody> {
    fn from(error: OhttpProcessingError) -> Self {
        let (status, code, message) = match error {
            OhttpProcessingError::DecapsulationFailed => (
                http::StatusCode::UNPROCESSABLE_ENTITY,
                "OHTTP_DECAPSULATION_FAILED",
                "Failed to decapsulate OHTTP request",
            ),
            OhttpProcessingError::InvalidFormat(detail) => {
                (http::StatusCode::UNPROCESSABLE_ENTITY, "OHTTP_INVALID_FORMAT", detail)
            }
            OhttpProcessingError::BodyTooLarge => (
                http::StatusCode::PAYLOAD_TOO_LARGE,
                "OHTTP_BODY_TOO_LARGE",
                "Request body exceeds the size limit",
            ),
            OhttpProcessingError::BadRequestBody => (
                http::StatusCode::BAD_REQUEST,
                "OHTTP_INVALID_FORMAT",
                "Failed to read request body",
            ),
            OhttpProcessingError::InternalError(detail) => {
                (http::StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR", detail)
            }
        };
        let body = serde_json::json!({
            "error": { "code": code, "message": message }
        });
        http::Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, "application/json")
            .body(HttpBody::from(body.to_string()))
            .expect("error response builder should not fail")
    }
}
