use blockifier::state::errors::StateError;
use blockifier::transaction::errors::TransactionExecutionError;
use blockifier::versioned_constants::VersionedConstantsError;
use serde_json::Error as SerdeError;
use starknet_api::StarknetApiError;
use starknet_gateway::errors::RPCStateReaderError;
use thiserror::Error;

#[derive(Debug, Error)]
#[allow(clippy::enum_variant_names)]
pub enum ReexecutionError {
    #[error("Cannot discern chain ID from URL: {0}")]
    AmbiguousChainIdFromUrl(String),
    #[error(transparent)]
    Rpc(#[from] RPCStateReaderError),
    #[error(transparent)]
    Serde(#[from] SerdeError),
    #[error(transparent)]
    StarknetApi(#[from] StarknetApiError),
    #[error(transparent)]
    State(#[from] StateError),
    #[error(transparent)]
    TransactionExecutionError(#[from] TransactionExecutionError),
    #[error(transparent)]
    VersionedConstants(#[from] VersionedConstantsError),
}
