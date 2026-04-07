//! JSON-RPC error types for the proving service.
//!
//! Error codes follow Starknet RPC specification v0.10.
//!
//! When adding a new error type:
//! 1. Add a variant to [`ProverRpcError`] below.
//! 2. Add the error to the OpenRPC spec: `resources/proving_api_openrpc.json` (both
//!    `components/errors` and the method's `errors` array).
//! 3. Run `test_error_responses_match_spec` — it will fail if (1) and (2) are out of sync.

use jsonrpsee::types::error::ErrorCode::InternalError;
use jsonrpsee::types::error::INTERNAL_ERROR_MSG;
use jsonrpsee::types::ErrorObjectOwned;
use strum::IntoEnumIterator;

use crate::errors::{
    ProofProviderError,
    RunnerError,
    VirtualBlockExecutorError,
    VirtualSnosProverError,
};

/// All documented JSON-RPC errors that `starknet_proveTransaction` can return.
///
/// Adding a variant here without updating the OpenRPC spec (or vice-versa) will
/// cause `test_error_responses_match_spec` to fail.
#[derive(strum::EnumIter)]
pub enum ProverRpcError {
    BlockNotFound,
    TransactionExecutionError,
    StorageProofNotSupported,
    AccountValidationFailed,
    UnsupportedTxVersion,
    ServiceBusy,
    InvalidTransactionInput,
}

impl ProverRpcError {
    /// The error code sent in the JSON-RPC response.
    pub fn code(&self) -> i32 {
        match self {
            Self::BlockNotFound => 24,
            Self::TransactionExecutionError => 41,
            Self::StorageProofNotSupported => 42,
            Self::AccountValidationFailed => 55,
            Self::UnsupportedTxVersion => 61,
            Self::ServiceBusy => -32005,
            Self::InvalidTransactionInput => 1000,
        }
    }

    /// The error message sent in the JSON-RPC response.
    pub fn message(&self) -> &'static str {
        match self {
            Self::BlockNotFound => "Block not found",
            Self::TransactionExecutionError => "Transaction execution error",
            Self::StorageProofNotSupported => {
                "The node doesn't support storage proofs for blocks that are too far in the past"
            }
            Self::AccountValidationFailed => "Account validation failed",
            Self::UnsupportedTxVersion => "The transaction version is not supported",
            Self::ServiceBusy => "Service is busy",
            Self::InvalidTransactionInput => "Invalid transaction input",
        }
    }

    /// Whether this error carries a `data` field in the JSON-RPC response.
    pub fn has_data(&self) -> bool {
        match self {
            Self::BlockNotFound | Self::StorageProofNotSupported => false,
            Self::TransactionExecutionError
            | Self::AccountValidationFailed
            | Self::UnsupportedTxVersion
            | Self::ServiceBusy
            | Self::InvalidTransactionInput => true,
        }
    }

    /// The key used in the OpenRPC spec's `components/errors` object.
    pub fn spec_key(&self) -> &'static str {
        match self {
            Self::BlockNotFound => "BLOCK_NOT_FOUND",
            Self::TransactionExecutionError => "TRANSACTION_EXECUTION_ERROR",
            Self::StorageProofNotSupported => "STORAGE_PROOF_NOT_SUPPORTED",
            Self::AccountValidationFailed => "ACCOUNT_VALIDATION_FAILED",
            Self::UnsupportedTxVersion => "UNSUPPORTED_TX_VERSION",
            Self::ServiceBusy => "SERVICE_BUSY",
            Self::InvalidTransactionInput => "INVALID_TRANSACTION_INPUT",
        }
    }

    /// Build an [`ErrorObjectOwned`] with optional data.
    pub fn to_error_object(&self, data: Option<String>) -> ErrorObjectOwned {
        ErrorObjectOwned::owned(self.code(), self.message(), data)
    }

    /// Returns an iterator over all variants.
    pub fn iter() -> ProverRpcErrorIter {
        <Self as IntoEnumIterator>::iter()
    }
}

// Convenience helpers used by the error-conversion logic below.

pub fn block_not_found() -> ErrorObjectOwned {
    ProverRpcError::BlockNotFound.to_error_object(None)
}

pub fn validation_failure(data: String) -> ErrorObjectOwned {
    ProverRpcError::AccountValidationFailed.to_error_object(Some(data))
}

pub fn unsupported_tx_version(data: String) -> ErrorObjectOwned {
    ProverRpcError::UnsupportedTxVersion.to_error_object(Some(data))
}

pub fn invalid_transaction_input(data: String) -> ErrorObjectOwned {
    ProverRpcError::InvalidTransactionInput.to_error_object(Some(data))
}

pub fn service_busy(max_concurrent: usize) -> ErrorObjectOwned {
    ProverRpcError::ServiceBusy.to_error_object(Some(format!(
        "The proving service is at capacity ({max_concurrent} concurrent request(s)). Please \
         retry later."
    )))
}

pub fn transaction_execution_error(data: String) -> ErrorObjectOwned {
    ProverRpcError::TransactionExecutionError.to_error_object(Some(data))
}

pub fn storage_proof_not_supported() -> ErrorObjectOwned {
    ProverRpcError::StorageProofNotSupported.to_error_object(None)
}

/// Creates an internal server error with the given message.
pub fn internal_server_error(err: impl std::fmt::Display) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(InternalError.code(), INTERNAL_ERROR_MSG, Some(err.to_string()))
}

/// Maps a [`RunnerError`] to a JSON-RPC error, surfacing known upstream error
/// codes instead of hiding them behind -32603.
fn runner_error_to_rpc(err: RunnerError) -> ErrorObjectOwned {
    match err {
        RunnerError::VirtualBlockExecutor(VirtualBlockExecutorError::UpstreamExecutionError(
            detail,
        )) => transaction_execution_error(detail),
        RunnerError::VirtualBlockExecutor(VirtualBlockExecutorError::TransactionReverted(
            _,
            ref reason,
        )) if reason.contains("Out of gas") => invalid_transaction_input(
            "Transaction reverted: out of gas. This is likely caused by l2_gas.max_amount being \
             too low. Set it to the value from starknet_estimateFee, or use 100000000 (0x5f5e100) \
             as a safe upper bound (sufficient for ~1 million Cairo steps)."
                .to_string(),
        ),
        RunnerError::ProofProvider(ProofProviderError::UpstreamRpcError { code, message }) => {
            let rpc_code = i32::try_from(code).unwrap_or(InternalError.code());
            ErrorObjectOwned::owned(rpc_code, message, None::<()>)
        }
        other => internal_server_error(other),
    }
}

impl From<VirtualSnosProverError> for ErrorObjectOwned {
    fn from(err: VirtualSnosProverError) -> Self {
        match err {
            VirtualSnosProverError::InvalidTransactionType(msg) => unsupported_tx_version(msg),
            VirtualSnosProverError::InvalidTransactionInput(msg) => invalid_transaction_input(msg),
            VirtualSnosProverError::ValidationError(msg) => {
                // Check if it's a pending block error.
                if msg.contains("Pending") { block_not_found() } else { validation_failure(msg) }
            }
            VirtualSnosProverError::RunnerError(e) => runner_error_to_rpc(*e),
            #[cfg(feature = "stwo_proving")]
            VirtualSnosProverError::ProvingError(e) => internal_server_error(e),
            VirtualSnosProverError::OutputParseError(e) => internal_server_error(e),
            VirtualSnosProverError::ProgramOutputError(e) => internal_server_error(e),
        }
    }
}
