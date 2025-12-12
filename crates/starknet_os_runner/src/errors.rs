use blockifier_reexecution::errors::ReexecutionError;
use starknet_rust::providers::ProviderError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SimulationError {
    #[error(transparent)]
    ReexecutionError(#[from] ReexecutionError),
}

#[derive(Debug, Error)]
pub enum ProofProviderError {
    #[error(transparent)]
    ProviderError(#[from] ProviderError),
}