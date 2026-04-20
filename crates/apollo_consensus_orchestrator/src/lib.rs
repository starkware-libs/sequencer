#![warn(missing_docs)]
//! An orchestrator for a StarkNet node.
//! Implements the consensus context - the interface for consensus to call out to the node.

#[allow(missing_docs)]
pub mod sequencer_consensus_context;

#[allow(missing_docs)]
pub mod build_proposal;

#[allow(missing_docs)]
pub mod validate_proposal;

/// Centralized and decentralized communication types and functionality.
#[allow(missing_docs)]
pub mod cende;

/// Fee market logic.
pub mod fee_market;

/// SNIP-35 dynamic L2 gas pricing (consensus-level fee mechanism).
pub mod snip35;

#[allow(missing_docs)]
pub mod metrics;

pub(crate) mod utils;

#[cfg(test)]
pub(crate) mod test_utils;

#[cfg(test)]
mod snip35_integration_test;
