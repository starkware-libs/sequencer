use blockifier::blockifier::transaction_executor::TransactionExecutorError;
use blockifier::blockifier_versioned_constants::VersionedConstantsError;
use blockifier::state::errors::StateError;
use blockifier::transaction::errors::TransactionExecutionError;
use reqwest::StatusCode;
use serde_json::{Error as SerdeError, Value};
use starknet_api::block::GasPrice;
use starknet_api::StarknetApiError;
use thiserror::Error;

use crate::state_reader::rpc_objects::{RpcErrorCode, RpcErrorResponse};

#[derive(Debug, Error)]
pub enum RPCStateReaderError {
    #[error("Block not found for request {0}")]
    BlockNotFound(Value),
    #[error("Class hash not found for request {0}")]
    ClassHashNotFound(Value),
    #[error("Contract address not found for request {0}")]
    ContractAddressNotFound(Value),
    #[error("Failed to parse gas price {:?}", 0)]
    GasPriceParsingFailure(GasPrice),
    #[error("Invalid params: {0:?}")]
    InvalidParams(Box<RpcErrorResponse>),
    #[error("RPC error: {0}")]
    RPCError(StatusCode),
    #[error("Transaction execution error: {0:?}")]
    TransactionExecutionError(Box<RpcErrorResponse>),
    #[error(transparent)]
    ReqwestError(#[from] reqwest::Error),
    #[error("Unexpected RPC error (code {code}): {message}{}", data.as_ref().map_or(String::new(), |d| format!(", data: {d}")))]
    UnexpectedErrorCode { code: RpcErrorCode, message: String, data: Option<Value> },
    #[error(transparent)]
    StarknetApi(#[from] StarknetApiError),
    #[error("Internal error: {0}")]
    InternalError(String),
}

pub type RPCStateReaderResult<T> = Result<T, RPCStateReaderError>;

impl From<RPCStateReaderError> for StateError {
    fn from(err: RPCStateReaderError) -> Self {
        match err {
            RPCStateReaderError::ClassHashNotFound(request) => {
                match serde_json::from_value(request["params"]["class_hash"].clone()) {
                    Ok(class_hash) => StateError::UndeclaredClassHash(class_hash),
                    Err(e) => serde_err_to_state_err(e),
                }
            }
            _ => StateError::StateReadError(err.to_string()),
        }
    }
}

// Converts a serde error to the error type of the state reader.
pub fn serde_err_to_state_err(err: SerdeError) -> StateError {
    StateError::StateReadError(format!("Failed to parse rpc result {:?}", err.to_string()))
}

#[derive(Debug, Error)]
#[allow(clippy::enum_variant_names)]
pub enum ReexecutionError {
    #[error("Cannot discern chain ID from URL: {0}")]
    AmbiguousChainIdFromUrl(String),
    #[error(
        "Block execution incomplete: {execution_info_count} execution infos for {tx_count} \
         transactions; cannot build complete transaction hashing data for block hash comparison"
    )]
    IncompleteBlockExecution { tx_count: usize, execution_info_count: usize },
    #[error(transparent)]
    Rpc(#[from] RPCStateReaderError),
    #[error(transparent)]
    Serde(#[from] SerdeError),
    #[error(transparent)]
    StarknetApi(#[from] StarknetApiError),
    #[error(transparent)]
    State(#[from] StateError),
    #[error(transparent)]
    TransactionExecutorError(#[from] TransactionExecutorError),
    #[error(transparent)]
    TransactionExecutionError(#[from] TransactionExecutionError),
    #[error(transparent)]
    VersionedConstants(#[from] VersionedConstantsError),
}

pub type ReexecutionResult<T> = Result<T, ReexecutionError>;
