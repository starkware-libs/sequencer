#![warn(missing_docs)]
// TODO(Matan): Add links to the spec.
// TODO(Matan): fix #[allow(missing_docs)].
//! A consensus implementation for a [Starknet](https://www.starknet.io/) node. The consensus
//! algorithm is based on [Tendermint](https://arxiv.org/pdf/1807.04938).
//!
//! Consensus communicates with other nodes via a gossip network; sending and receiving votes on one
//! topic and streaming proposals on a separate topic. [details](https://github.com/starknet-io/starknet-p2p-specs/tree/main/p2p/proto/consensus).
//!
//! In addition to the network inputs, consensus reaches out to the rest of the node via the
//! [`Context`](types::ConsensusContext) API.
//!
//! Consensus is generic over the content of the proposals, and merely requires an identifier to be
//! produced by the Context.
//!
//! Consensus operates in two modes:
//! 1. Observer - Receives consensus messages and updates the node when a decision is reached.
//! 2. Active - In addition to receiving messages, the node can also send messages to the network.
//!
//! Observer mode offers lower latency compared to sync, as Proposals and votes are processed in
//! real-time rather than after a decision has been made.
//!
//! Consensus is an active component, it doesn't follow the server/client model:
//! 1. The outbound messages are not sent as responses to the inbound messages.
//! 2. It generates and runs its own events (e.g. timeouts).

pub mod config;
#[allow(missing_docs)]
pub mod types;
pub use manager::{run_consensus, RunConsensusArguments};
#[allow(missing_docs)]
pub mod metrics;
#[allow(missing_docs)]
pub mod simulation_network_receiver;
pub mod stream_handler;

mod manager;
#[allow(missing_docs)]
mod single_height_consensus;
#[allow(missing_docs)]
mod state_machine;
#[allow(missing_docs)]
pub mod votes_threshold;

#[cfg(test)]
pub(crate) mod test_utils;
