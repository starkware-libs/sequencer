#![warn(missing_docs)]
//! An orchestrator for a StarkNet node.
//! Implements the consensus context - the interface for consensus to call out to the node.

#[allow(missing_docs)]
pub mod sequencer_consensus_context;

/// Centralized and decentralized communication types and functionality.
#[allow(missing_docs)]
pub mod cende;

/// Fee market logic.
pub mod fee_market;

/// Consensus' versioned constants.
pub mod orchestrator_versioned_constants;

/// The orchestrator's configuration.
pub mod config;

#[allow(missing_docs)]
mod metrics;
