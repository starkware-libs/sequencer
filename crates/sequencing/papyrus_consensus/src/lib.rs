#![warn(missing_docs)]
// TODO(Matan): Add a description of the crate.
// TODO(Matan): fix #[allow(missing_docs)].
//! A consensus implementation for a [`Starknet`](https://www.starknet.io/) node.

pub mod config;
pub mod manager;
#[allow(missing_docs)]
pub mod simulation_network_receiver;
#[allow(missing_docs)]
pub mod single_height_consensus;
#[allow(missing_docs)]
pub mod state_machine;
pub mod stream_handler;
#[cfg(test)]
pub(crate) mod test_utils;
#[allow(missing_docs)]
pub mod types;

pub use manager::run_consensus;
