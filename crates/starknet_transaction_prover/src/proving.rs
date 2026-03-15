//! Proof generation from Cairo PIE outputs.
//!
//! Orchestrates the Stwo prover and converts raw prover output into proof facts suitable for
//! on-chain verification.

#[cfg(feature = "stwo_proving")]
pub(crate) mod prover;
pub mod virtual_snos_prover;

#[cfg(all(test, feature = "stwo_proving"))]
mod prover_test;
#[cfg(test)]
mod virtual_snos_prover_test;
