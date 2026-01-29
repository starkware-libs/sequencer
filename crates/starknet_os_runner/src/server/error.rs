//! JSON-RPC error types for the proving service.
//!
//! Error codes follow Starknet RPC specification v0.10.

use jsonrpsee::types::error::ErrorCode::InternalError;
use jsonrpsee::types::error::INTERNAL_ERROR_MSG;
use jsonrpsee::types::ErrorObjectOwned;
use serde::Serialize;

use crate::virtual_snos_prover::VirtualSnosProverError;

/// A JSON-RPC error with optional data.
#[derive(Clone, Debug)]
pub struct JsonRpcError<T: Serialize> {
    pub code: i32,
    pub message: &'static str,
    pub data: Option<T>,
}

// Starknet RPC v0.10 error codes.

/// Block not found (code 24).
pub const BLOCK_NOT_FOUND: JsonRpcError<String> =
    JsonRpcError { code: 24, message: "Block not found", data: None };

/// Invalid transaction hash (code 25).
pub const INVALID_TXN_HASH: JsonRpcError<String> =
    JsonRpcError { code: 25, message: "Invalid transaction hash", data: None };

/// Account validation failed (code 55).
pub fn validation_failure(data: String) -> JsonRpcError<String> {
    JsonRpcError { code: 55, message: "Account validation failed", data: Some(data) }
}

/// Unsupported transaction version (code 61).
pub const UNSUPPORTED_TX_VERSION: JsonRpcError<String> =
    JsonRpcError { code: 61, message: "The transaction version is not supported", data: None };

impl<T: Serialize> From<JsonRpcError<T>> for ErrorObjectOwned {
    fn from(err: JsonRpcError<T>) -> Self {
        ErrorObjectOwned::owned(err.code, err.message, err.data)
    }
}

/// Creates an internal server error with the given message.
pub fn internal_server_error(err: impl std::fmt::Display) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(InternalError.code(), INTERNAL_ERROR_MSG, Some(err.to_string()))
}

impl From<VirtualSnosProverError> for ErrorObjectOwned {
    fn from(err: VirtualSnosProverError) -> Self {
        match &err {
            VirtualSnosProverError::InvalidTransactionType(msg) => {
                // Use UNSUPPORTED_TX_VERSION but include the specific message as data.
                ErrorObjectOwned::owned(
                    UNSUPPORTED_TX_VERSION.code,
                    UNSUPPORTED_TX_VERSION.message,
                    Some(msg.clone()),
                )
            }
            VirtualSnosProverError::ValidationError(msg) => {
                // Check if it's a pending block error.
                if msg.contains("Pending") {
                    BLOCK_NOT_FOUND.into()
                } else {
                    validation_failure(msg.clone()).into()
                }
            }
            VirtualSnosProverError::TransactionHashError(msg) => ErrorObjectOwned::owned(
                INVALID_TXN_HASH.code,
                INVALID_TXN_HASH.message,
                Some(msg.clone()),
            ),
            VirtualSnosProverError::RunnerError(e) => internal_server_error(e),
            VirtualSnosProverError::ProvingError(e) => internal_server_error(e),
            VirtualSnosProverError::OutputParseError(e) => internal_server_error(e),
            VirtualSnosProverError::ProgramOutputError(e) => internal_server_error(e),
        }
    }
}
