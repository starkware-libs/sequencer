//! Running module for executing transactions and generating OS input.
//!
//! This module contains all the logic for:
//! - Executing virtual blocks of transactions
//! - Fetching storage proofs
//! - Providing contract classes
//! - Running the virtual OS

pub mod classes_provider;
pub mod committer_utils;
pub mod runner;
pub mod storage_proofs;
pub mod virtual_block_executor;

#[cfg(test)]
mod classes_provider_test;
#[cfg(test)]
pub mod rpc_records;
#[cfg(test)]
mod rpc_records_test;
#[cfg(test)]
mod runner_test;
#[cfg(test)]
mod storage_proofs_test;
#[cfg(test)]
pub mod test_utils;
#[cfg(test)]
mod virtual_block_executor_test;
