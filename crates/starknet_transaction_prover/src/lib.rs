//! Standalone service that proves individual Starknet transactions using the virtual Starknet OS
//! and Stwo prover.
//!
//! The [`server`] module exposes the proving pipeline as a JSON-RPC service over HTTP or HTTPS
//! (with optional TLS). When a request arrives, it passes through two internal stages:
//!
//! 1. **Running** ([`running`]) — re-executes the transaction against the target block to collect
//!    execution data, storage proofs, and contract classes, then runs the Starknet virtual OS to
//!    produce a Cairo PIE.
//!
//! 2. **Proving** ([`proving`]) — feeds the Cairo PIE into the Stwo prover to generate a
//!    zero-knowledge proof and proof facts.
//!
//! # Feature flags
//!
//! * `stwo_proving` — enables in-memory Stwo proving (requires a nightly Rust toolchain).
//! * `cairo_native` — enables Cairo Native compilation via blockifier.

pub mod config;
pub mod errors;
pub mod proving;
pub mod running;
pub mod server;

#[cfg(feature = "stwo_proving")]
pub use proving::prover::prove_cairo_pie_standalone;

#[cfg(test)]
mod test_utils;
