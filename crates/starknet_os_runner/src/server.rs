pub mod config;
pub mod cors;
pub mod error;
pub mod rpc_impl;
pub mod rpc_trait;

#[cfg(test)]
#[path = "server/rpc_spec_test.rs"]
mod rpc_spec_test;
