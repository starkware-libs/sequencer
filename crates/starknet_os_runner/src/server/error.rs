//! JSON-RPC error types for the proving service.
//!
//! Error codes follow Starknet RPC specification v0.10.

use jsonrpsee::types::error::ErrorCode::InternalError;
use jsonrpsee::types::error::INTERNAL_ERROR_MSG;
use jsonrpsee::types::ErrorObjectOwned;

use crate::proving::virtual_snos_prover::VirtualSnosProverError;

// Starknet RPC v0.10 error codes.

/// Block not found (code 24).
pub fn block_not_found() -> ErrorObjectOwned {
    ErrorObjectOwned::owned(24, "Block not found", None::<()>)
}

/// Account validation failed (code 55).
pub fn validation_failure(data: String) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(55, "Account validation failed", Some(data))
}

/// Unsupported transaction version (code 61).
pub fn unsupported_tx_version(data: String) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(61, "The transaction version is not supported", Some(data))
}

/// Service is busy — too many concurrent proving requests (code -32005).
pub fn service_busy(max_concurrent: usize) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(
        -32005,
        "Service is busy",
        Some(format!(
            "The proving service is at capacity ({max_concurrent} concurrent request(s)). Please \
             retry later."
        )),
    )
}

/// Creates an internal server error with the given message.
pub fn internal_server_error(err: impl std::fmt::Display) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(InternalError.code(), INTERNAL_ERROR_MSG, Some(err.to_string()))
}

impl From<VirtualSnosProverError> for ErrorObjectOwned {
    fn from(err: VirtualSnosProverError) -> Self {
        match &err {
            VirtualSnosProverError::InvalidTransactionType(msg) => {
                unsupported_tx_version(msg.clone())
            }
            VirtualSnosProverError::ValidationError(msg) => {
                // Check if it's a pending block error.
                if msg.contains("Pending") {
                    block_not_found()
                } else {
                    validation_failure(msg.clone())
                }
            }
            VirtualSnosProverError::RunnerError(e) => internal_server_error(e),
            VirtualSnosProverError::ProvingError(e) => internal_server_error(e),
            VirtualSnosProverError::OutputParseError(e) => internal_server_error(e),
            VirtualSnosProverError::ProgramOutputError(e) => internal_server_error(e),
        }
    }
}
