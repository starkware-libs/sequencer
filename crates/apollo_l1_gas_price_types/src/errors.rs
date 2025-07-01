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
    JoinError(#[from] tokio::task::JoinError),
    #[error(transparent)]
    RequestError(#[from] reqwest::Error),
    // #[error(transparent)]
    // TimeoutError(#[from] tokio::time::error::Elapsed),
    #[error(transparent)]
    ParseError(#[from] serde_json::Error),
    #[error("Missing or invalid field: {0}")]
    MissingFieldError(&'static str),
    #[error("Invalid decimals value: expected {0}, got {1}")]
    InvalidDecimalsError(u64, u64),
    #[error("Query not yet resolved: timestamp={0}")]
    QueryNotReadyError(u64),
    #[error("All URLs in the list failed for timestamp {0}")]
    AllUrlsFailedError(u64, usize),
}
