//! Configuration types for Apollo consensus.
//!
//! This crate contains configuration structures used by the consensus system,
//! including `ConsensusConfig`, `TimeoutsConfig`, and `StreamHandlerConfig`.

pub mod config;

// Re-export the main types for convenience
pub use config::{ConsensusConfig, StreamHandlerConfig, TimeoutsConfig};

// Re-export ValidatorId type for convenience
pub type ValidatorId = starknet_api::core::ContractAddress;
