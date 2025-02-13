use thiserror::Error;

#[derive(Clone, Debug, Error)]
pub enum L1GasPriceProviderError {
    #[error("Failed to add price info: {0}")]
    InvalidHeight(String),
    #[error("Failed to calculate price info: {0}")]
    MissingData(String),
}
