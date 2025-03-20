use serde::{Deserialize, Serialize};
use starknet_sequencer_infra::component_client::ClientError;
use thiserror::Error;

#[derive(Clone, Debug, Error, PartialEq, Eq, Serialize, Deserialize)]
pub enum L1GasPriceProviderError {
    #[error("Block height is not consecutive: expected {expected}, got {found}")]
    UnexpectedHeightError { expected: u64, found: u64 },
    #[error("No price data saved for blocks starting at {timestamp} - {lag} seconds")]
    MissingDataError { timestamp: u64, lag: u64 },
    #[error("Insufficient block price history: expected at least {expected}, found only {found}")]
    InsufficientHistoryError { expected: usize, found: usize },
}

#[derive(Clone, Debug, Error)]
pub enum L1GasPriceClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    L1GasPriceProviderError(#[from] L1GasPriceProviderError),
}

#[derive(Debug, Error)]
pub enum EthToStrkOracleClientError {
    #[error(transparent)]
    RequestError(#[from] reqwest::Error),
    #[error(transparent)]
    ParseError(#[from] serde_json::Error),
    #[error("Missing or invalid field: {0}")]
    MissingFieldError(&'static str),
    #[error("Invalid decimals value: expected {0}, got {1}")]
    InvalidDecimalsError(u64, u64),
}
