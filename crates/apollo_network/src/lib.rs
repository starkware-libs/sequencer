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
//! - `discovery`: Peer discovery mechanisms (Kademlia DHT, bootstrapping)
//! - `peer_manager`: Peer lifecycle and reputation management
//! - [`misconduct_score`]: Peer reputation scoring system
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use apollo_network::network_manager::metrics::NetworkMetrics;
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

#[cfg(test)]
mod config_test;
pub mod discovery;
#[cfg(test)]
mod e2e_broadcast_test;
pub mod gossipsub_impl;
pub mod misconduct_score;
mod mixed_behaviour;
pub mod network_manager;
pub mod peer_manager;
mod sqmr;
#[cfg(test)]
mod test_utils;
pub mod utils;

use std::collections::{BTreeMap, HashSet};
use std::str::FromStr;
use std::time::Duration;

use apollo_config::converters::{
    deserialize_optional_vec_u8,
    deserialize_seconds_to_duration,
    serialize_optional_vec_u8,
};
use apollo_config::dumping::{
    prepend_sub_config_name,
    ser_optional_param,
    ser_param,
    SerializeConfig,
};
use apollo_config::validators::validate_vec_u256;
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use discovery::DiscoveryConfig;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::Multiaddr;
use peer_manager::PeerManagerConfig;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};
use starknet_api::core::ChainId;
use validator::{Validate, ValidationError};

pub(crate) type Bytes = Vec<u8>;

/// Deserializes a comma-separated string of multiaddresses into an optional vector.
///
/// This function provides custom deserialization for bootstrap peer multiaddresses.
/// It handles the following cases:
/// - Empty string `""` → `None`
/// - Single address `"addr1"` → `Some(vec![addr1])`
/// - Multiple addresses `"addr1,addr2"` → `Some(vec![addr1, addr2])`
///
/// This design maintains backward compatibility with older configurations that
/// supported only a single bootstrap peer.
///
/// # Arguments
///
/// * `de` - The deserializer to read from
///
/// # Returns
///
/// * `Ok(None)` for empty strings
/// * `Ok(Some(vec))` for valid multiaddress strings
/// * `Err(D::Error)` for invalid multiaddress formats
///
/// # Examples
///
/// ```text
/// ""                                           → None
/// "/ip4/1.2.3.4/tcp/10000/p2p/12D3KooW..."   → Some(vec![addr1])
/// "/ip4/1.2.3.4/tcp/10000/p2p/...,/ip4/..."  → Some(vec![addr1, addr2])
/// ```
fn deserialize_multi_addrs<'de, D>(de: D) -> Result<Option<Vec<Multiaddr>>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw_str: String = Deserialize::deserialize(de).unwrap_or_default();
    if raw_str.is_empty() {
        return Ok(None);
    }

    let mut vector = Vec::new();
    for i in raw_str.split(',').filter(|s| !s.is_empty()) {
        let value = Multiaddr::from_str(i).map_err(|_| {
            D::Error::custom(format!("Couldn't deserialize vector. Failed to parse value: {i}"))
        })?;
        vector.push(value);
    }

    if vector.is_empty() {
        return Ok(None);
    }

    Ok(Some(vector))
}

/// Serializes an optional vector of multiaddresses to a comma-separated string.
///
/// This function provides the inverse operation of [`deserialize_multi_addrs`],
/// converting multiaddresses back to their string representation for serialization.
///
/// # Arguments
///
/// * `multi_addrs` - Optional vector of multiaddresses to serialize
///
/// # Returns
///
/// * Empty string for `None`
/// * Comma-separated string of multiaddresses for `Some(vec)`
///
/// # Examples
///
/// ```text
/// None                     → ""
/// Some(vec![addr1])        → "/ip4/1.2.3.4/tcp/10000/p2p/12D3KooW..."
/// Some(vec![addr1, addr2]) → "/ip4/1.2.3.4/tcp/10000/p2p/...,/ip4/..."
/// ```
fn serialize_multi_addrs(multi_addrs: &Option<Vec<Multiaddr>>) -> String {
    match multi_addrs {
        None => "".to_owned(),
        Some(multi_addrs) => multi_addrs
            .iter()
            .map(|multiaddr| multiaddr.to_string())
            .collect::<Vec<String>>()
            .join(","),
    }
}

/// Network configuration for the Apollo networking layer.
///
/// This struct contains all the configuration parameters needed to initialize and run
/// the networking subsystem. It includes network-level settings, protocol configurations,
/// and various timeout and buffer size parameters.
///
/// # Examples
///
/// ## Basic Configuration
///
/// ```rust
/// use std::time::Duration;
///
/// use apollo_network::NetworkConfig;
/// use starknet_api::core::ChainId;
///
/// let config = NetworkConfig {
///     port: 10000,
///     chain_id: ChainId::Mainnet,
///     session_timeout: Duration::from_secs(120),
///     ..Default::default()
/// };
/// ```
///
/// ## Configuration with Bootstrap Peers
///
/// ```rust
/// use apollo_network::NetworkConfig;
/// use libp2p::Multiaddr;
/// use starknet_api::core::ChainId;
///
/// let bootstrap_peer = "/ip4/1.2.3.4/tcp/10000/p2p/\
///                       12D3KooWQYHvEJzuBPEXdwMfVdPGXeEFSioa7YcXqWn5Ey6qM8q7"
///     .parse()
///     .unwrap();
/// let config = NetworkConfig {
///     port: 10000,
///     chain_id: ChainId::Mainnet,
///     bootstrap_peer_multiaddr: Some(vec![bootstrap_peer]),
///     ..Default::default()
/// };
/// ```
///
/// # Validation
///
/// The configuration is automatically validated when deserialized or when
/// `validate()` is called. Validation includes:
/// - Ensuring bootstrap peer multiaddresses contain valid peer IDs
/// - Checking that bootstrap peer IDs are unique
/// - Validating the secret key format if provided
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Validate)]
pub struct NetworkConfig {
    /// The port number the node listens on for incoming connections.
    ///
    /// This is the TCP port that the libp2p swarm will bind to for accepting
    /// inbound connections from other peers in the network.
    ///
    /// Default: 10000
    ///
    /// ```
    /// # use apollo_network::NetworkConfig;
    /// let config = NetworkConfig::default();
    /// assert_eq!(config.port, 10000);
    /// ```
    pub port: u16,

    /// Maximum time a session can remain active before timing out.
    ///
    /// This applies to both inbound and outbound SQMR sessions. Sessions that
    /// exceed this duration will be automatically terminated.
    ///
    /// Default: 120 seconds
    ///
    /// ```
    /// # use apollo_network::NetworkConfig;
    /// # use std::time::Duration;
    /// let config = NetworkConfig::default();
    /// assert_eq!(config.session_timeout, Duration::from_secs(120));
    /// ```
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub session_timeout: Duration,

    /// Maximum time a connection can remain idle before being closed.
    ///
    /// Connections with no active sessions will be terminated after this duration
    /// to conserve system resources.
    ///
    /// Default: 120 seconds
    ///
    /// ```
    /// # use apollo_network::NetworkConfig;
    /// # use std::time::Duration;
    /// let config = NetworkConfig::default();
    /// assert_eq!(config.idle_connection_timeout, Duration::from_secs(120));
    /// ```
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub idle_connection_timeout: Duration,

    /// List of bootstrap peer multiaddresses for initial network connectivity.
    ///
    /// These are the initial peers that the node will attempt to connect to when
    /// starting up. Each multiaddress must include a valid peer ID. The peer IDs
    /// must be unique across all bootstrap peers.
    ///
    /// Format: `/ip4/1.2.3.4/tcp/10000/p2p/<peer-id>`
    ///
    /// Default: None (no bootstrap peers)
    ///
    /// ```
    /// # use apollo_network::NetworkConfig;
    /// let config = NetworkConfig::default();
    /// assert_eq!(config.bootstrap_peer_multiaddr, None);
    /// ```
    #[serde(deserialize_with = "deserialize_multi_addrs")]
    #[validate(custom(function = "validate_bootstrap_peer_multiaddr_list"))]
    pub bootstrap_peer_multiaddr: Option<Vec<Multiaddr>>,

    /// Optional secret key for deterministic peer ID generation.
    ///
    /// If provided, this key will be used to generate the node's peer ID deterministically.
    /// If None, a random key will be generated on each startup, resulting in a different
    /// peer ID each time.
    ///
    /// The key should be a 32-byte Ed25519 private key.
    ///
    /// Default: None (random key generation)
    ///
    /// ```
    /// # use apollo_network::NetworkConfig;
    /// let config = NetworkConfig::default();
    /// assert_eq!(config.secret_key, None);
    /// ```
    #[validate(custom = "validate_vec_u256")]
    #[serde(deserialize_with = "deserialize_optional_vec_u8")]
    pub secret_key: Option<Vec<u8>>,

    /// Optional external multiaddress that other peers should use to reach this node.
    ///
    /// If set, this address will be advertised to other peers instead of the
    /// automatically detected external addresses. This is useful when the node
    /// is behind NAT or when you want to control exactly which address is advertised.
    ///
    /// Default: None (automatic address detection)
    ///
    /// ```
    /// # use apollo_network::NetworkConfig;
    /// let config = NetworkConfig::default();
    /// assert_eq!(config.advertised_multiaddr, None);
    /// ```
    pub advertised_multiaddr: Option<Multiaddr>,

    /// The Starknet chain ID this node operates on.
    ///
    /// This is used to ensure that the node only connects to peers on the same
    /// network (mainnet, testnet, etc.).
    ///
    /// Default: ChainId::Mainnet
    ///
    /// ```
    /// # use apollo_network::NetworkConfig;
    /// # use starknet_api::core::ChainId;
    /// let config = NetworkConfig::default();
    /// assert_eq!(config.chain_id, ChainId::Mainnet);
    /// ```
    pub chain_id: ChainId,

    /// Configuration for peer discovery mechanisms.
    ///
    /// This includes settings for Kademlia DHT bootstrapping, heartbeat intervals,
    /// and retry policies for peer discovery operations.
    pub discovery_config: DiscoveryConfig,

    /// Configuration for peer lifecycle and reputation management.
    ///
    /// This controls how peers are managed throughout their lifecycle, including
    /// connection limits, reputation scoring, and peer eviction policies.
    pub peer_manager_config: PeerManagerConfig,

    /// Buffer size for broadcasted message metadata.
    ///
    /// This controls the size of the internal buffer that holds metadata about
    /// messages that have been broadcast through the gossipsub network. A larger
    /// buffer can handle more concurrent broadcast messages but uses more memory.
    ///
    /// Default: 100000
    ///
    /// ```
    /// # use apollo_network::NetworkConfig;
    /// let config = NetworkConfig::default();
    /// assert_eq!(config.broadcasted_message_metadata_buffer_size, 100000);
    /// ```
    pub broadcasted_message_metadata_buffer_size: usize,

    /// Buffer size for reported peer IDs.
    ///
    /// This controls the size of the internal buffer that holds peer IDs that have
    /// been reported for malicious behavior. Reported peers are subject to reputation
    /// penalties and potential disconnection.
    ///
    /// Default: 100000
    ///
    /// ```
    /// # use apollo_network::NetworkConfig;
    /// let config = NetworkConfig::default();
    /// assert_eq!(config.reported_peer_ids_buffer_size, 100000);
    /// ```
    pub reported_peer_ids_buffer_size: usize,
}

impl SerializeConfig for NetworkConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = BTreeMap::from_iter([
            ser_param(
                "port",
                &self.port,
                "The port that the node listens on for incoming connections.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "session_timeout",
                &self.session_timeout.as_secs(),
                "Maximal time in seconds that each session can take before failing on timeout.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "idle_connection_timeout",
                &self.idle_connection_timeout.as_secs(),
                "Amount of time in seconds that a connection with no active sessions will stay \
                 alive.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "chain_id",
                &self.chain_id,
                "The chain to follow. For more details see https://docs.starknet.io/documentation/architecture_and_concepts/Blocks/transactions/#chain-id.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "broadcasted_message_metadata_buffer_size",
                &self.broadcasted_message_metadata_buffer_size,
                "The size of the buffer that holds the metadata of the broadcasted messages.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "reported_peer_ids_buffer_size",
                &self.reported_peer_ids_buffer_size,
                "The size of the buffer that holds the reported peer ids.",
                ParamPrivacyInput::Public,
            ),
        ]);
        config.extend(ser_optional_param(
            &if self.bootstrap_peer_multiaddr.is_some(){
                Some(serialize_multi_addrs(&self.bootstrap_peer_multiaddr))
            } else {
                None
            },
            String::from(""),
            "bootstrap_peer_multiaddr",
            "The multiaddress of the peer node. It should include the peer's id. For more info: https://docs.libp2p.io/concepts/fundamentals/peers/",
            ParamPrivacyInput::Public,
        ));
        config.extend([ser_param(
            "secret_key",
            &serialize_optional_vec_u8(&self.secret_key),
            "The secret key used for building the peer id. If it's an empty string a random one \
             will be used.",
            ParamPrivacyInput::Private,
        )]);
        config.extend(ser_optional_param(
            &self.advertised_multiaddr,
            Multiaddr::empty(),
            "advertised_multiaddr",
            "The external address other peers see this node. If this is set, the node will not \
             try to find out which addresses it has and will write this address as external \
             instead",
            ParamPrivacyInput::Public,
        ));
        config.extend(prepend_sub_config_name(self.discovery_config.dump(), "discovery_config"));
        config.extend(prepend_sub_config_name(
            self.peer_manager_config.dump(),
            "peer_manager_config",
        ));
        config
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            port: 10000,
            session_timeout: Duration::from_secs(120),
            idle_connection_timeout: Duration::from_secs(120),
            bootstrap_peer_multiaddr: None,
            secret_key: None,
            advertised_multiaddr: None,
            chain_id: ChainId::Mainnet,
            discovery_config: DiscoveryConfig::default(),
            peer_manager_config: PeerManagerConfig::default(),
            broadcasted_message_metadata_buffer_size: 100000,
            reported_peer_ids_buffer_size: 100000,
        }
    }
}

/// Validates a list of bootstrap peer multiaddresses.
///
/// This function ensures that:
/// 1. Each multiaddress contains a valid peer ID
/// 2. All peer IDs in the list are unique
/// 3. The multiaddresses are properly formatted
///
/// # Arguments
///
/// * `bootstrap_peer_multiaddr` - A slice of multiaddresses to validate
///
/// # Returns
///
/// * `Ok(())` if all validations pass
/// * `Err(ValidationError)` if any validation fails
///
/// # Examples
///
/// Valid bootstrap peers:
/// ```text
/// /ip4/1.2.3.4/tcp/10000/p2p/12D3KooWQYHvEJzuBP...
/// /ip6/::1/tcp/10000/p2p/12D3KooWDifferentPeer...
/// ```
///
/// Invalid (missing peer ID):
/// ```text
/// /ip4/1.2.3.4/tcp/10000
/// ```
///
/// Invalid (duplicate peer ID):
/// ```text
/// /ip4/1.2.3.4/tcp/10000/p2p/12D3KooWSamePeer...
/// /ip4/5.6.7.8/tcp/10000/p2p/12D3KooWSamePeer...
/// ```
fn validate_bootstrap_peer_multiaddr_list(
    bootstrap_peer_multiaddr: &[Multiaddr],
) -> Result<(), validator::ValidationError> {
    let mut peers = HashSet::new();
    for address in bootstrap_peer_multiaddr.iter() {
        let Some(peer_id) = DialOpts::from(address.clone()).get_peer_id() else {
            return Err(ValidationError::new(
                "Bootstrap peer Multiaddr does not contain a PeerId.",
            ));
        };

        if !peers.insert(peer_id) {
            let mut error = ValidationError::new("Bootstrap peer PeerIds are not unique.");
            error.message = Some(std::borrow::Cow::from(format!("Repeated PeerId: {peer_id}")));
            return Err(error);
        }
    }
    Ok(())
}
