//! JSON-RPC server exposing the proving pipeline.
//!
//! Provides the HTTP entry point, concurrency limiting, CORS configuration, and error mapping
//! from internal prover errors to JSON-RPC error codes.

pub mod config;
pub mod cors;
pub mod errors;
pub mod mock_rpc;
pub mod rpc_api;
pub mod rpc_impl;
