#![warn(missing_docs)]
//! An orchestrator for a StarkNet node.
//! Implements the consensus context - the interface for consensus to call out to the node.

#[expect(missing_docs)]
pub mod sequencer_consensus_context;

#[expect(missing_docs)]
pub mod build_proposal;

#[expect(missing_docs)]
pub mod validate_proposal;

/// Centralized and decentralized communication types and functionality.
#[expect(missing_docs)]
pub mod cende;

/// Fee market logic.
pub mod fee_market;

/// Dynamic L2 gas pricing (consensus-level fee mechanism).
pub mod dynamic_gas_price;

#[expect(missing_docs)]
pub mod metrics;

pub(crate) mod utils;

#[cfg(test)]
pub(crate) mod test_utils;
