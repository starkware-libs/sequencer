//! JSON-RPC error types for the proving service.
//!
//! Error codes follow Starknet RPC specification v0.10.
//!
//! Service-defined errors are declared as [`ServiceErrorCode`] variants — add new errors there.
//! The spec conformance test (`server/rpc_spec_test.rs`, `test_error_responses_match_spec`)
//! iterates the enum and fails until the OpenRPC spec in starknet-specs
//! (`proving-api/starknet_proving_api_openrpc.json`) documents the new error.

use jsonrpsee::types::error::ErrorCode::InternalError;
use jsonrpsee::types::error::INTERNAL_ERROR_MSG;
use jsonrpsee::types::ErrorObjectOwned;
#[cfg(test)]
use strum::EnumIter;

use crate::errors::{
    ProofProviderError,
    RunnerError,
    VirtualBlockExecutorError,
    VirtualSnosProverError,
};

/// Every JSON-RPC error the proving service itself defines, one variant per error documented in
/// the proving-api OpenRPC spec. Each error's code and canonical message live only here; the
/// constructor functions below are thin wrappers. The service can also return the standard
/// JSON-RPC internal error (-32603) and pass-through upstream Starknet errors, which are not
/// spec-enumerated and deliberately not variants.
#[derive(Clone, Copy)]
#[cfg_attr(test, derive(EnumIter))]
pub(crate) enum ServiceErrorCode {
    BlockNotFound,
    AccountValidationFailed,
    InvalidTransactionInput,
    UnsupportedTxType,
    /// Blocked by the external compliance check.
    TransactionBlocked,
    ServiceBusy,
}

impl ServiceErrorCode {
    pub(crate) fn code(self) -> i32 {
        match self {
            Self::BlockNotFound => 24,
            Self::AccountValidationFailed => 55,
            Self::InvalidTransactionInput => 1000,
            Self::UnsupportedTxType => 1001,
            Self::TransactionBlocked => 10000,
            Self::ServiceBusy => -32005,
        }
    }

    fn message(self) -> &'static str {
        match self {
            Self::BlockNotFound => "Block not found",
            Self::AccountValidationFailed => "Account validation failed",
            Self::InvalidTransactionInput => "Invalid transaction input",
            Self::UnsupportedTxType => "the transaction type is not supported",
            Self::TransactionBlocked => "Transaction blocked",
            Self::ServiceBusy => "Service is busy",
        }
    }

    fn error_object(self, data: Option<String>) -> ErrorObjectOwned {
        ErrorObjectOwned::owned(self.code(), self.message(), data)
    }
}

pub fn block_not_found() -> ErrorObjectOwned {
    ServiceErrorCode::BlockNotFound.error_object(None)
}

pub fn validation_failure(data: String) -> ErrorObjectOwned {
    ServiceErrorCode::AccountValidationFailed.error_object(Some(data))
}

pub fn unsupported_tx_type(data: String) -> ErrorObjectOwned {
    ServiceErrorCode::UnsupportedTxType.error_object(Some(data))
}

pub fn invalid_transaction_input(data: String) -> ErrorObjectOwned {
    ServiceErrorCode::InvalidTransactionInput.error_object(Some(data))
}

pub fn transaction_blocked() -> ErrorObjectOwned {
    ServiceErrorCode::TransactionBlocked.error_object(None)
}

pub fn service_busy(max_concurrent: usize) -> ErrorObjectOwned {
    ServiceErrorCode::ServiceBusy.error_object(Some(format!(
        "The proving service is at capacity ({max_concurrent} concurrent request(s)). Please \
         retry later."
    )))
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
        other => internal_server_error(other),
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
            VirtualSnosProverError::ProvingError(e) => internal_server_error(e),
            VirtualSnosProverError::OutputParseError(e) => internal_server_error(e),
            VirtualSnosProverError::ProgramOutputError(e) => internal_server_error(e),
            VirtualSnosProverError::TransactionBlocked => transaction_blocked(),
        }
    }
}
