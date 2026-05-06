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

pub use self::retry::RetryConfig;
pub use self::starknet_error::{KnownStarknetErrorCode, StarknetError, StarknetErrorCode};
pub use crate::reader::ClientError;

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

