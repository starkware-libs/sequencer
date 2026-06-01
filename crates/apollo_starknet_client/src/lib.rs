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
pub use crate::reader::{ClientCreationError, ClientError, RetryErrorCode};
