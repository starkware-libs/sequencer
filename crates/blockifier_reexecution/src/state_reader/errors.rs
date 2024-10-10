use blockifier::state::errors::StateError;
use blockifier::transaction::errors::TransactionExecutionError;
use serde_json::Error as SerdeError;
use starknet_gateway::errors::RPCStateReaderError;
use thiserror::Error;

#[derive(Debug, Error)]
#[allow(clippy::enum_variant_names)]
pub enum ReexecutionError {
    #[error(transparent)]
    StateError(#[from] StateError),
    #[error(transparent)]
    RPCError(#[from] RPCStateReaderError),
    #[error(transparent)]
    SerdeError(#[from] SerdeError),
    #[error(transparent)]
    TransactionExecutionError(#[from] TransactionExecutionError),
    /// Represents all unexpected errors that may occur while reading from state.
    #[error("Failed to read from state: {0}.")]
    StateReadError(String),
}
