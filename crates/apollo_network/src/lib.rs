//! # Apollo Network
//!
//! Apollo Network is a comprehensive peer-to-peer networking crate that provides networking
//! capabilities for Starknet sequencer nodes. It implements the [Starknet P2P specifications]
//! and offers a robust, scalable networking layer built on top of [libp2p].
//!
//! ## Features
//!
//! - **SQMR Protocol**: Single Query Multiple Response protocol for efficient peer communication
//! - **GossipSub Broadcasting**: Reliable message broadcasting across the network
//! - **Peer Discovery**: Automatic peer discovery using Kademlia DHT and bootstrapping
//! - **Network Management**: Comprehensive connection and session management
//! - **Metrics & Monitoring**: Built-in metrics collection and monitoring capabilities
//! - **Configurable**: Extensive configuration options for various network parameters
//!
//! ## Architecture Overview
//!
//! The crate is organized into several key modules:
//!
//! - [`network_manager`]: Core networking functionality and the main entry point
//! - `sqmr`: Single Query Multiple Response protocol implementation
//! - [`gossipsub_impl`]: GossipSub-based message broadcasting
//! - [`discovery`]: Peer discovery mechanisms (Kademlia DHT, bootstrapping)
//! - [`peer_manager`]: Peer lifecycle and reputation management
//! - [`misconduct_score`]: Peer reputation scoring system
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use apollo_network::metrics::NetworkMetrics;
//! use apollo_network::network_manager::NetworkManager;
//! use apollo_network::NetworkConfig;
//! use starknet_api::core::ChainId;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create network configuration
//! let config = NetworkConfig { port: 10000, chain_id: ChainId::Mainnet, ..Default::default() };
//!
//! // Initialize network manager
//! let network_manager = NetworkManager::new(
//!     config,
//!     Some("apollo-node/0.1.0".to_string()),
//!     None, // metrics
//! );
//!
//! // Run the network manager
//! network_manager.run().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Protocol Implementation
//!
//! ### SQMR (Single Query Multiple Response)
//!
//! SQMR enables efficient request-response communication patterns where a single query
//! can receive multiple responses from peers. This is particularly useful for data
//! synchronization and block/transaction propagation.
//!
//! ```rust,no_run
//! # use apollo_network::network_manager::NetworkManager;
//! # use futures::StreamExt;
//! # use serde::{Serialize, Deserialize};
//! #
//! # // Example types for demonstration
//! # #[derive(Serialize, Deserialize, Clone)]
//! # struct Query { id: u64 }
//! # impl TryFrom<Vec<u8>> for Response {
//! #     type Error = Box<dyn std::error::Error + Send + Sync>;
//! #     fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
//! #         Ok(Response { data: String::from_utf8(bytes)? })
//! #     }
//! # }
//! # impl From<Query> for Vec<u8> {
//! #     fn from(query: Query) -> Vec<u8> { query.id.to_string().into_bytes() }
//! # }
//! # #[derive(Serialize, Deserialize, Clone)]
//! # struct Response { data: String }
//! #
//! # async fn example(mut network_manager: NetworkManager) -> Result<(), Box<dyn std::error::Error>> {
//! // Register as a client for a protocol
//! let mut client = network_manager.register_sqmr_protocol_client::<Query, Response>(
//!     "/starknet/blocks/1.0.0".to_string(),
//!     1000, // buffer size
//! );
//!
//! // Send query and receive responses
//! let query = Query { id: 123 };
//! let mut response_manager = client.send_new_query(query).await?;
//! while let Some(response) = response_manager.next().await {
//!     match response {
//!         Ok(response) => {
//!             // Process response
//!             println!("Got response: {}", response.data);
//!         }
//!         Err(e) => {
//!             // Handle error, optionally report peer
//!             response_manager.report_peer();
//!             break;
//!         }
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ### GossipSub Broadcasting
//!
//! GossipSub provides efficient message broadcasting with configurable propagation
//! and validation policies.
//!
//! ```rust,no_run
//! # use apollo_network::network_manager::{NetworkManager, BroadcastTopicClientTrait};
//! # use apollo_network::gossipsub_impl::Topic;
//! # use futures::StreamExt;
//! # use serde::{Serialize, Deserialize};
//! #
//! # // Example transaction type for demonstration
//! # #[derive(Serialize, Deserialize, Clone)]
//! # struct Transaction { hash: String, amount: u64 }
//! # impl TryFrom<Vec<u8>> for Transaction {
//! #     type Error = Box<dyn std::error::Error + Send + Sync>;
//! #     fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
//! #         Ok(Transaction { hash: String::from_utf8(bytes)?, amount: 100 })
//! #     }
//! # }
//! # impl From<Transaction> for Vec<u8> {
//! #     fn from(tx: Transaction) -> Vec<u8> { tx.hash.into_bytes() }
//! # }
//! #
//! # async fn example(mut network_manager: NetworkManager) -> Result<(), Box<dyn std::error::Error>> {
//! // Register for a broadcast topic
//! let topic = Topic::new("transactions");
//! let mut channels = network_manager.register_broadcast_topic::<Transaction>(topic, 1000)?;
//!
//! // Broadcast messages
//! let transaction = Transaction { hash: "tx123".to_string(), amount: 100 };
//! channels.broadcast_topic_client.broadcast_message(transaction).await?;
//!
//! // Receive broadcasted messages
//! while let Some((result, metadata)) = channels.broadcasted_messages_receiver.next().await {
//!     match result {
//!         Ok(transaction) => {
//!             // Process transaction
//!             println!("Received transaction: {}", transaction.hash);
//!             // Continue propagation if valid
//!             channels.broadcast_topic_client.continue_propagation(&metadata).await?;
//!         }
//!         Err(e) => {
//!             // Report malicious peer
//!             channels.broadcast_topic_client.report_peer(metadata).await?;
//!         }
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Configuration
//!
//! The [`NetworkConfig`] struct provides extensive configuration options:
//!
//! ```rust
//! use std::time::Duration;
//!
//! use apollo_network::discovery::DiscoveryConfig;
//! use apollo_network::peer_manager::PeerManagerConfig;
//! use apollo_network::NetworkConfig;
//! use starknet_api::core::ChainId;
//!
//! let config = NetworkConfig {
//!     port: 10000,
//!     session_timeout: Duration::from_secs(120),
//!     idle_connection_timeout: Duration::from_secs(120),
//!     chain_id: ChainId::Mainnet,
//!     discovery_config: DiscoveryConfig::default(),
//!     peer_manager_config: PeerManagerConfig::default(),
//!     ..Default::default()
//! };
//! ```
//!
//! ## Error Handling
//!
//! The crate provides comprehensive error handling through the `NetworkError` enum
//! and appropriate error propagation for all network operations.
//!
//! ## Thread Safety
//!
//! All public APIs are designed to work in async/await contexts and are thread-safe
//! where appropriate. The network manager handles all low-level networking concerns
//! internally.
//!
//! [Starknet P2P specifications]: https://github.com/starknet-io/starknet-p2p-specs/
//! [libp2p]: https://libp2p.io/

pub mod active_committees;
#[cfg(test)]
mod config_test;
pub mod discovery;
mod event_tracker;
pub mod gossipsub_impl;
pub mod metrics;
pub mod misconduct_score;
#[cfg(any(test, feature = "testing"))]
pub mod mixed_behaviour;
#[cfg(not(any(test, feature = "testing")))]
mod mixed_behaviour;
pub mod network_manager;
pub mod peer_manager;
mod peer_whitelist;
#[cfg(test)]
mod peer_whitelist_test;
#[cfg(any(test, feature = "testing"))]
pub mod prune_dead_connections;
#[cfg(not(any(test, feature = "testing")))]
mod prune_dead_connections;
pub mod sqmr;
#[cfg(test)]
mod test_utils;
pub mod utils;

#[cfg(any(test, feature = "testing"))]
pub type Bytes = Vec<u8>;
#[cfg(not(any(test, feature = "testing")))]
pub(crate) type Bytes = Vec<u8>;

pub use apollo_network_config::NetworkConfig;
