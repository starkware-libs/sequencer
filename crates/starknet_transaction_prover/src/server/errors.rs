//! JSON-RPC error types for the proving service.
//!
//! Error codes follow Starknet RPC specification v0.10.
//!
//! When adding a new error type, also update:
//! - The OpenRPC spec in starknet-specs: `proving-api/starknet_proving_api_openrpc.json`
//! - The spec validation test: `server/rpc_spec_test.rs` (`test_error_responses_match_spec`)

use blockifier_reexecution::errors::{RPCStateReaderError, ReexecutionError};
use jsonrpsee::types::error::ErrorCode::InternalError;
use jsonrpsee::types::error::INTERNAL_ERROR_MSG;
use jsonrpsee::types::ErrorObjectOwned;

#[cfg(feature = "stwo_proving")]
use crate::errors::ProvingError;
use crate::errors::{
    ClassesProviderError,
    ProofProviderError,
    RunnerError,
    VirtualBlockExecutorError,
    VirtualSnosProverError,
};

#[cfg(test)]
#[path = "errors_test.rs"]
mod errors_test;

// Starknet RPC v0.10 error codes.

/// Block not found (code 24).
pub fn block_not_found() -> ErrorObjectOwned {
    ErrorObjectOwned::owned(24, "Block not found", None::<()>)
}

/// Account validation failed (code 55).
pub fn validation_failure(data: String) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(55, "Account validation failed", Some(data))
}

/// Unsupported transaction type (code 1001).
pub fn unsupported_tx_type(data: String) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(1001, "the transaction type is not supported", Some(data))
}

/// Invalid transaction input (code 1000).
pub fn invalid_transaction_input(data: String) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(1000, "Invalid transaction input", Some(data))
}

/// Transaction uses a builtin the prover does not support (code 1002).
pub fn unsupported_builtin(data: String) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(1002, "Unsupported builtin", Some(data))
}

/// Transaction blocked by external compliance check (code 10000).
pub fn transaction_blocked() -> ErrorObjectOwned {
    ErrorObjectOwned::owned(10000, "Transaction blocked", None::<()>)
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

/// Troubleshooting hint appended to out-of-gas errors. The most common cause is users picking a
/// gas amount that's too small for their transaction; this points them at `starknet_estimateFee`
/// or a safe upper bound.
const OUT_OF_GAS_HINT: &str = "This is likely caused by l2_gas.max_amount being too low. Set it \
                               to the value from starknet_estimateFee, or use 100000000 \
                               (0x5f5e100) as a safe upper bound (sufficient for ~1 million Cairo \
                               steps).";

/// Builds an out-of-gas error response, prefixing the upstream `reason` so callers see the
/// original revert message together with the troubleshooting hint.
fn out_of_gas_error(reason: &str) -> ErrorObjectOwned {
    invalid_transaction_input(format!("{reason}\n\n{OUT_OF_GAS_HINT}"))
}

/// Maps a [`RunnerError`] to a JSON-RPC error, surfacing known upstream error
/// codes instead of hiding them behind -32603.
///
/// The client sees the upstream message of [`ProofProviderError::UpstreamRpcError`], the revert
/// reason of [`VirtualBlockExecutorError::TransactionReverted`], and the text of
/// [`ProofProviderError::InvalidStateDiff`], [`ClassesProviderError::DeprecatedContractError`],
/// [`VirtualBlockExecutorError::TransactionExecutionError`] and
/// [`RPCStateReaderError::BlockNotFound`] from the virtual block executor. Every other error gets a
/// fixed message that names the failing step. Transport errors among them embed the node URL,
/// whose path or query can hold an API key.
fn runner_error_to_rpc(err: RunnerError) -> ErrorObjectOwned {
    match err {
        RunnerError::VirtualBlockExecutor(VirtualBlockExecutorError::TransactionReverted(
            _,
            ref reason,
        )) if reason.contains("Out of gas") => out_of_gas_error(reason),
        RunnerError::ProofProvider(ProofProviderError::UpstreamRpcError {
            code,
            message,
            data,
        }) => {
            let rpc_code = i32::try_from(code).unwrap_or(InternalError.code());
            if rpc_code >= 0 {
                // Positive codes are user-facing Starknet application errors — forward the
                // upstream code, message, and any data (e.g. nonce details for code 41) as-is.
                ErrorObjectOwned::owned(rpc_code, message, data)
            } else {
                // Negative codes are JSON-RPC infrastructure errors — hide behind -32603.
                internal_server_error(format!(
                    "Upstream JSON-RPC error (code {rpc_code}): {message}"
                ))
            }
        }
        // The text tells the client which state change the virtual OS does not allow.
        RunnerError::ProofProvider(state_diff_error @ ProofProviderError::InvalidStateDiff(_)) => {
            internal_server_error(state_diff_error)
        }
        RunnerError::ProofProvider(
            ProofProviderError::Rpc(_)
            | ProofProviderError::SerializationError(_)
            | ProofProviderError::InvalidProofResponse(_)
            | ProofProviderError::BlockCommitmentError(_),
        ) => internal_server_error("Failed to get storage proofs"),
        // Its text names only a class hash and tells the client why the transaction cannot be
        // proven.
        RunnerError::ClassesProvider(
            deprecated_class_error @ ClassesProviderError::DeprecatedContractError(_),
        ) => internal_server_error(deprecated_class_error),
        RunnerError::ClassesProvider(
            ClassesProviderError::GetClassesError(_)
            | ClassesProviderError::InvalidBytecodeElement
            | ClassesProviderError::StateError(_)
            | ClassesProviderError::HintsConversionError(_),
        ) => internal_server_error("Failed to load contract classes"),
        // The client chose the block ID. The text holds the node request body, which has no URL.
        RunnerError::VirtualBlockExecutor(VirtualBlockExecutorError::ReexecutionError(
            ref reexecution_error,
        )) if matches!(
            **reexecution_error,
            ReexecutionError::Rpc(RPCStateReaderError::BlockNotFound(_))
        ) =>
        {
            internal_server_error(reexecution_error)
        }
        RunnerError::VirtualBlockExecutor(VirtualBlockExecutorError::ReexecutionError(_)) => {
            internal_server_error("Failed to read block state")
        }
        RunnerError::VirtualBlockExecutor(
            VirtualBlockExecutorError::StateUnavailable
            | VirtualBlockExecutorError::BouncerLockError(_),
        ) => internal_server_error("Virtual block execution failed"),
        // Clients need blockifier's execution and revert text to debug their transaction.
        RunnerError::VirtualBlockExecutor(
            execution_error @ (VirtualBlockExecutorError::TransactionExecutionError(_)
            | VirtualBlockExecutorError::TransactionReverted(..)),
        ) => internal_server_error(execution_error),
        RunnerError::OsExecution(_) => internal_server_error("Starknet OS execution failed"),
        RunnerError::InputGenerationError(_) => internal_server_error("OS input generation failed"),
        RunnerError::TransactionHashError(_) => {
            internal_server_error("Failed to calculate the transaction hash")
        }
        RunnerError::TaskJoin(_) => internal_server_error("Virtual block execution task failed"),
    }
}

impl From<VirtualSnosProverError> for ErrorObjectOwned {
    fn from(err: VirtualSnosProverError) -> Self {
        match err {
            VirtualSnosProverError::InvalidTransactionType(msg) => unsupported_tx_type(msg),
            VirtualSnosProverError::InvalidTransactionInput(msg) => invalid_transaction_input(msg),
            VirtualSnosProverError::ValidationError(msg) => {
                // Check if it's a pending block error.
                if msg.contains("Pending") { block_not_found() } else { validation_failure(msg) }
            }
            VirtualSnosProverError::RunnerError(e) => runner_error_to_rpc(*e),
            #[cfg(feature = "stwo_proving")]
            VirtualSnosProverError::ProvingError(
                error @ ProvingError::UnsupportedBuiltins { .. },
            ) => unsupported_builtin(error.to_string()),
            #[cfg(feature = "stwo_proving")]
            VirtualSnosProverError::ProvingError(e) => internal_server_error(e),
            VirtualSnosProverError::OutputParseError(e) => internal_server_error(e),
            VirtualSnosProverError::ProgramOutputError(e) => internal_server_error(e),
            VirtualSnosProverError::TransactionBlocked => transaction_blocked(),
        }
    }
}
