//! JSON-RPC error types for the proving service.
//!
//! Error codes follow Starknet RPC specification v0.10.
//!
//! When adding a new error type, also update:
//! - The OpenRPC spec: `resources/proving_api_openrpc.json` (under `components/errors`)
//! - The spec validation test: `server/rpc_spec_test.rs` (`test_error_responses_match_spec`)

use jsonrpsee::types::error::ErrorCode::InternalError;
use jsonrpsee::types::error::INTERNAL_ERROR_MSG;
use jsonrpsee::types::ErrorObjectOwned;

use crate::errors::{ProofProviderError, RunnerError, VirtualBlockExecutorError, VirtualSnosProverError};

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

/// Invalid transaction input (code 1000).
pub fn invalid_transaction_input(data: String) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(1000, "Invalid transaction input", Some(data))
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

/// Transaction execution error (code 41).
pub fn transaction_execution_error(data: String) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(41, "Transaction execution error", Some(data))
}

/// Storage proof not supported for this block (code 42).
pub fn storage_proof_not_supported() -> ErrorObjectOwned {
    ErrorObjectOwned::owned(
        42,
        "The node doesn't support storage proofs for blocks that are too far in the past",
        None::<()>,
    )
}

/// Creates an internal server error with the given message.
pub fn internal_server_error(err: impl std::fmt::Display) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(InternalError.code(), INTERNAL_ERROR_MSG, Some(err.to_string()))
}

/// Maps a [`RunnerError`] to a JSON-RPC error, surfacing known upstream error
/// codes instead of hiding them behind -32603.
fn runner_error_to_rpc(err: RunnerError) -> ErrorObjectOwned {
    match err {
        RunnerError::VirtualBlockExecutor(
            VirtualBlockExecutorError::UpstreamExecutionError(detail),
        ) => transaction_execution_error(detail),
        RunnerError::ProofProvider(ProofProviderError::UpstreamRpcError { code, message }) => {
            let rpc_code =
                i32::try_from(code).unwrap_or(InternalError.code());
            ErrorObjectOwned::owned(rpc_code, message, None::<()>)
        }
        other => internal_server_error(other),
    }
}

impl From<VirtualSnosProverError> for ErrorObjectOwned {
    fn from(err: VirtualSnosProverError) -> Self {
        match err {
            VirtualSnosProverError::InvalidTransactionType(msg) => {
                unsupported_tx_version(msg)
            }
            VirtualSnosProverError::InvalidTransactionInput(msg) => {
                invalid_transaction_input(msg)
            }
            VirtualSnosProverError::ValidationError(msg) => {
                // Check if it's a pending block error.
                if msg.contains("Pending") {
                    block_not_found()
                } else {
                    validation_failure(msg)
                }
            }
            VirtualSnosProverError::RunnerError(e) => runner_error_to_rpc(*e),
            #[cfg(feature = "stwo_proving")]
            VirtualSnosProverError::ProvingError(e) => internal_server_error(e),
            VirtualSnosProverError::OutputParseError(e) => internal_server_error(e),
            VirtualSnosProverError::ProgramOutputError(e) => internal_server_error(e),
        }
    }
}
