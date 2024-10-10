use blockifier::state::errors::StateError;
use serde_json::Error as SerdeError;
use starknet_gateway::errors::RPCStateReaderError;
use thiserror::Error;

#[derive(Debug, Error)]
#[allow(clippy::enum_variant_names)]
pub enum ReexecutionError {
    #[error(transparent)]
    FromStateError(#[from] StateError),
    #[error(transparent)]
    FromRPCError(#[from] RPCStateReaderError),
    #[error("Failed to read from state: {0}.")]
    StateReadError(String),
}

// Converts a serde error to the error type of the state reader.
pub fn serde_err_to_reexecution_err(err: SerdeError) -> ReexecutionError {
    ReexecutionError::StateReadError(format!("Failed to parse rpc result {:?}", err.to_string()))
}
