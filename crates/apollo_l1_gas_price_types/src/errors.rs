use apollo_infra::component_client::ClientError;
use serde::{Deserialize, Serialize};
use strum::{EnumDiscriminants, EnumIter, IntoStaticStr, VariantNames};
use thiserror::Error;

use crate::CurrencyPair;

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
    ExchangeRateOracleClientError(#[from] ExchangeRateOracleClientError),
}

#[derive(Clone, Debug, Error)]
pub enum L1GasPriceClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    L1GasPriceProviderError(#[from] L1GasPriceProviderError),
    #[error(transparent)]
    ExchangeRateOracleClientError(#[from] ExchangeRateOracleClientError),
}

#[derive(Clone, Debug, Error, Serialize, Deserialize, PartialEq, Eq, EnumDiscriminants)]
#[strum_discriminants(
    name(ExchangeRateOracleErrorType),
    derive(IntoStaticStr, EnumIter, VariantNames),
    strum(serialize_all = "snake_case")
)]
pub enum ExchangeRateOracleClientError {
    #[error("Join error: {0}")]
    JoinError(String),
    #[error("Request error: {0}")]
    RequestError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Missing or invalid field: {0}. Body: {1}")]
    MissingFieldError(String, String),
    #[error("Invalid decimals value: expected {0}, got {1}")]
    InvalidDecimalsError(u32, u32),
    #[error("Query not yet resolved: timestamp={0}")]
    QueryNotReadyError(u64),
    #[error("All URLs in the list failed for timestamp {0}, starting with index {1}")]
    AllUrlsFailedError(u64, usize),
    #[error("Invalid rate from oracle: {0}")]
    InvalidRateError(String),
    // [Temporary comment] The five variants below are first returned by the feed guards (A7/A8).
    // TODO(Asaf): give the remaining variants the same pair context. Not a payload field for all
    // of them: `From<reqwest::Error>` and the `?` sites have no pair to supply, so this
    // likely needs a span field on `fetch_rate` instead.
    #[error(
        "Stale {pair} price feed: last updated at {updated_at}, priced for block timestamp \
         {block_timestamp}, maximum accepted staleness is {max_staleness_seconds} seconds"
    )]
    StaleFeedError {
        pair: CurrencyPair,
        updated_at: u64,
        block_timestamp: u64,
        max_staleness_seconds: u64,
    },
    #[error(
        "The {pair} price feed is dated {updated_at}, more than {max_future_updated_at_seconds} \
         seconds ahead of the block timestamp {block_timestamp}"
    )]
    FutureFeedError {
        pair: CurrencyPair,
        updated_at: u64,
        block_timestamp: u64,
        max_future_updated_at_seconds: u64,
    },
    #[error("Rate {rate} for {pair} is outside the accepted range [{min_rate}, {max_rate}]")]
    RateOutOfBoundsError { pair: CurrencyPair, rate: u128, min_rate: u128, max_rate: u128 },
    #[error("Contract call to price feed failed: {0}")]
    ContractCallError(String),
    #[error("Arithmetic overflow while computing rate: {0}")]
    ArithmeticError(String),
}

impl From<reqwest::Error> for ExchangeRateOracleClientError {
    fn from(value: reqwest::Error) -> Self {
        ExchangeRateOracleClientError::RequestError(value.to_string())
    }
}
