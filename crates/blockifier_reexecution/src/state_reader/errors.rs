use blockifier::state::errors::StateError;
use blockifier::transaction::errors::TransactionExecutionError;
use blockifier::versioned_constants::VersionedConstantsError;
use serde_json::Error as SerdeError;
use starknet_gateway::errors::RPCStateReaderError;
use thiserror::Error;

#[derive(Debug, Error)]
#[allow(clippy::enum_variant_names)]
pub enum ReexecutionError {
    #[error(transparent)]
    State(#[from] StateError),
    #[error(transparent)]
    Rpc(#[from] RPCStateReaderError),
    #[error(transparent)]
    Serde(#[from] SerdeError),
    #[error(transparent)]
    TransactionExecutionError(#[from] TransactionExecutionError),
    /// Represents all unexpected errors that may occur while reading from state.
    #[error(transparent)]
    VersionedConstants(#[from] VersionedConstantsError),
}
