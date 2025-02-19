use serde::{Deserialize, Serialize};
use starknet_sequencer_infra::component_client::ClientError;
use thiserror::Error;

#[derive(Clone, Debug, Error, PartialEq, Eq, Serialize, Deserialize)]
pub enum L1GasPriceProviderError {
    #[error("Failed to add price info: {0}")]
    InvalidHeight(String),
    #[error("Failed to add price info: {0}")]
    MissingData(String),
    #[error("Failed to get price info: {0}")]
    GetPriceInfoError(String),
}

#[derive(Clone, Debug, Error)]
pub enum L1GasPriceClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    L1GasPriceProviderError(#[from] L1GasPriceProviderError),
}
