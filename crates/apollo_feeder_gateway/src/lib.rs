//! Feeder gateway read API server for the Apollo sequencer.

pub mod communication;
pub mod eip55;
pub mod errors;
pub mod feeder_gateway;
pub mod handlers;
pub(crate) mod legacy_params;
pub mod metrics;
pub mod objects;
pub mod reader;
pub mod serialization;

#[cfg(test)]
#[path = "felt_format_lock_test.rs"]
mod felt_format_lock_test;

#[cfg(test)]
#[path = "transaction_parity_test.rs"]
mod transaction_parity_test;
