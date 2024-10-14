use blockifier::state::errors::StateError;
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
    VersionedConstants(#[from] VersionedConstantsError),
}
