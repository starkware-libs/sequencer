// config compiler to support coverage_attribute feature when running coverage in nightly mode
// within this crate
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

//! This crate contains clients that can communicate with [`Starknet`] through the various
//! endpoints [`Starknet`] has.
//!
//!
//! [`Starknet`]: https://starknet.io/

pub mod reader;
pub mod retry;
pub mod starknet_error;
#[cfg(test)]
mod test_utils;

use reqwest::StatusCode;

pub use self::retry::RetryConfig;
pub use self::starknet_error::{KnownStarknetErrorCode, StarknetError, StarknetErrorCode};

/// Errors that might be encountered while creating the client.
#[derive(thiserror::Error, Debug)]
pub enum ClientCreationError {
    #[error(transparent)]
    BadUrl(#[from] url::ParseError),
    #[error(transparent)]
    BuildError(#[from] reqwest::Error),
    #[error("Failed to create header map.")]
    HttpHeaderError,
}

/// Errors that might be solved by retrying mechanism.
#[derive(Debug, Eq, PartialEq)]
pub enum RetryErrorCode {
    Redirect,
    Timeout,
    TooManyRequests,
    ServiceUnavailable,
    Disconnect,
}

/// Errors that may be returned by a client.
#[derive(thiserror::Error, Debug)]
pub enum ClientError {
    /// A client error representing bad status http responses.
    #[error("Bad response status code: {:?} message: {:?}.", code, message)]
    BadResponseStatus { code: StatusCode, message: String },
    /// A client error representing http request errors.
    #[error(transparent)]
    RequestError(#[from] reqwest::Error),
    /// A client error representing errors that might be solved by retrying mechanism.
    #[error("Retry error code: {:?}, message: {:?}.", code, message)]
    RetryError { code: RetryErrorCode, message: String },
    /// A client error representing deserialization errors.
    #[error(transparent)]
    SerdeError(#[from] serde_json::Error),
    /// A client error representing errors returned by the starknet client.
    #[error(transparent)]
    StarknetError(#[from] StarknetError),
}
