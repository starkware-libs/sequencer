//! Configuration types for Apollo consensus.
//!
//! This crate contains configuration structures used by the consensus system,
//! including `ConsensusConfig`, `TimeoutsConfig`, and `StreamHandlerConfig`.

pub mod config;

pub type ValidatorId = starknet_api::core::ContractAddress;
