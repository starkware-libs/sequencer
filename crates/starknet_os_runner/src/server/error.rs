//! JSON-RPC error types for the proving service.
//!
//! Error codes follow Starknet RPC specification v0.10.
//!
//! When adding a new error variant, also update:
//! - The OpenRPC spec: `resources/proving_api_openrpc.json` (under `components/errors`)
//! - The spec validation test: `server/rpc_spec_test.rs` (`error_responses_match_spec`)

use jsonrpsee::types::error::ErrorCode::InternalError;
use jsonrpsee::types::error::INTERNAL_ERROR_MSG;
use jsonrpsee::types::ErrorObjectOwned;

use crate::proving::virtual_snos_prover::VirtualSnosProverError;

/// RPC errors returned by the proving service.
pub enum RpcError {
    BlockNotFound,
    ValidationFailure(String),
    UnsupportedTxVersion(String),
    ServiceBusy(usize),
    InternalServerError(String),
}

impl RpcError {
    /// Returns all spec-defined error variants with sample data.
    ///
    /// Uses an exhaustive match internally so that adding a new variant without updating
    /// this list is a compile error.
    pub fn all_spec_variants() -> Vec<(&'static str, Self)> {
        let all = vec![
            ("BLOCK_NOT_FOUND", RpcError::BlockNotFound),
            ("ACCOUNT_VALIDATION_FAILED", RpcError::ValidationFailure("test".to_string())),
            ("UNSUPPORTED_TX_VERSION", RpcError::UnsupportedTxVersion("v99".to_string())),
            ("SERVICE_BUSY", RpcError::ServiceBusy(2)),
        ];

        // Exhaustive match ensures new variants cause a compile error here.
        for (_, variant) in &all {
            match variant {
                RpcError::BlockNotFound
                | RpcError::ValidationFailure(_)
                | RpcError::UnsupportedTxVersion(_)
                | RpcError::ServiceBusy(_)
                | RpcError::InternalServerError(_) => {}
            }
        }

        all
    }
}

impl From<RpcError> for ErrorObjectOwned {
    fn from(err: RpcError) -> Self {
        match err {
            RpcError::BlockNotFound => {
                ErrorObjectOwned::owned(24, "Block not found", None::<()>)
            }
            RpcError::ValidationFailure(data) => {
                ErrorObjectOwned::owned(55, "Account validation failed", Some(data))
            }
            RpcError::UnsupportedTxVersion(data) => {
                ErrorObjectOwned::owned(61, "The transaction version is not supported", Some(data))
            }
            RpcError::ServiceBusy(max_concurrent) => ErrorObjectOwned::owned(
                -32005,
                "Service is busy",
                Some(format!(
                    "The proving service is at capacity ({max_concurrent} concurrent request(s)). \
                     Please retry later."
                )),
            ),
            RpcError::InternalServerError(msg) => {
                ErrorObjectOwned::owned(InternalError.code(), INTERNAL_ERROR_MSG, Some(msg))
            }
        }
    }
}

impl From<VirtualSnosProverError> for ErrorObjectOwned {
    fn from(err: VirtualSnosProverError) -> Self {
        let rpc_error = match &err {
            VirtualSnosProverError::InvalidTransactionType(msg) => {
                RpcError::UnsupportedTxVersion(msg.clone())
            }
            VirtualSnosProverError::ValidationError(msg) => {
                // Check if it's a pending block error.
                if msg.contains("Pending") {
                    RpcError::BlockNotFound
                } else {
                    RpcError::ValidationFailure(msg.clone())
                }
            }
            VirtualSnosProverError::RunnerError(e) => {
                RpcError::InternalServerError(e.to_string())
            }
            #[cfg(feature = "stwo_proving")]
            VirtualSnosProverError::ProvingError(e) => {
                RpcError::InternalServerError(e.to_string())
            }
            VirtualSnosProverError::OutputParseError(e) => {
                RpcError::InternalServerError(e.to_string())
            }
            VirtualSnosProverError::ProgramOutputError(e) => {
                RpcError::InternalServerError(e.to_string())
            }
        };
        rpc_error.into()
    }
}
