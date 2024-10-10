use blockifier::state::errors::StateError;
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
    StateReadError(#[from] SerdeError),
}
