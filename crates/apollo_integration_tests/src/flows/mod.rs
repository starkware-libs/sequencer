//! End-to-end integration-test flows, one module per flow. Each flow orchestrates sequencer
//! nodes (spawned as child processes) end to end and is designed to run as its own process.

pub mod positive;
pub mod proof;
pub mod restart;
pub mod restart_multiple_nodes;
pub mod restart_single_node;
pub mod revert;
pub mod sync;
