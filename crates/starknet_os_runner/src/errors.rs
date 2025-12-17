use starknet_rust::providers::ProviderError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProofProviderError {
    #[error(transparent)]
    ProviderError(#[from] ProviderError),
}
