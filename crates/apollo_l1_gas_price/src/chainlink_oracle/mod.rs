//! Reading the Chainlink price feeds on Starknet.

pub mod contract_call_error;
pub mod feed_decode;
pub mod feed_math;
pub mod feed_read;

// [Temporary comment] A harness for a single test file so far; A9's `test.rs` shares it.
#[cfg(test)]
mod test_utils;
