use apollo_infra::component_client::ClientError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Error, PartialEq, Eq, Serialize, Deserialize)]
pub enum L1GasPriceProviderError {
    #[error("Block number is not consecutive: expected {expected}, got {found}")]
    UnexpectedBlockNumberError { expected: u64, found: u64 },
    #[error("No price data saved for blocks starting at {timestamp} - {lag} seconds")]
    MissingDataError { timestamp: u64, lag: u64 },
    #[error("Insufficient block price history: expected at least {expected}, found only {found}")]
    InsufficientHistoryError { expected: usize, found: usize },
    #[error("Price Provider is not initialized")]
    NotInitializedError,
    #[error(
        "Stale L1 gas prices: no new data received for {current_timestamp} - \
         {last_valid_price_timestamp} seconds"
    )]
    StaleL1GasPricesError { current_timestamp: u64, last_valid_price_timestamp: u64 },
    #[error(transparent)]
    EthToStrkOracleClientError(#[from] EthToStrkOracleClientError),
}

#[derive(Clone, Debug, Error)]
pub enum L1GasPriceClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    L1GasPriceProviderError(#[from] L1GasPriceProviderError),
    #[error(transparent)]
    EthToStrkOracleClientError(#[from] EthToStrkOracleClientError),
}

#[derive(Clone, Debug, Error, Serialize, Deserialize, PartialEq, Eq)]
pub enum EthToStrkOracleClientError {
    #[error("Join error: {0}")]
    JoinError(String),
    #[error("Request error: {0}")]
    RequestError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Missing or invalid field: {0}. Body: {1}")]
    MissingFieldError(String, String),
    #[error("Invalid decimals value: expected {0}, got {1}")]
    InvalidDecimalsError(u64, u64),
    #[error("Query not yet resolved: timestamp={0}")]
    QueryNotReadyError(u64),
    #[error("All URLs in the list failed for timestamp {0}, starting with index {1}")]
    AllUrlsFailedError(u64, usize),
}

impl From<reqwest::Error> for EthToStrkOracleClientError {
    fn from(value: reqwest::Error) -> Self {
        EthToStrkOracleClientError::RequestError(value.to_string())
    }
}
