use thiserror::Error;

#[derive(Debug, Error)]
pub enum L1ProviderRunError {
    #[error("L1 provider client failed to start: {0}")]
    L1ProviderClientError(String),
}
