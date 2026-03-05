//! Standalone service that proves individual Starknet transactions using the virtual Starknet OS and Stwo
//! prover.
//!
//! A JSON-RPC request flows through three stages:
//!
//! 1. **Running** ([`running`]) — re-executes the transaction against the target block to collect
//!    execution data, storage proofs, and contract classes, then runs the Starknet virtual OS to
//!    produce a Cairo PIE.
//!
//! 2. **Proving** ([`proving`]) — feeds the Cairo PIE into the Stwo prover to generate a
//!    zero-knowledge proof and proof facts.
//!
//! 3. **Server** ([`server`]) — exposes the proving pipeline over JSON-RPC, handles concurrency
//!    limiting, CORS, and error mapping.
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

#[cfg(test)]
mod test_utils;
