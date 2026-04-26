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

use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

use apollo_config::converters::{
    deserialize_optional_sensitive_vec_u8,
    deserialize_seconds_to_duration,
    deserialize_vec,
    serialize_optional_vec_u8,
    serialize_slice,
};
use apollo_config::dumping::{
    prepend_sub_config_name,
    ser_optional_param,
    ser_optional_sub_config,
    ser_param,
    SerializeConfig,
};
use apollo_config::secrets::Sensitive;
use apollo_config::validators::validate_optional_sensitive_vec_u256;
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_network_types::network_types::PeerId;
use discovery::DiscoveryConfig;
use libp2p::identity::Keypair;
use libp2p::multihash::Multihash;
use libp2p::Multiaddr;
use peer_manager::PeerManagerConfig;
use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;
use validator::{Validate, ValidationError};

use crate::prune_dead_connections::{DEFAULT_PING_INTERVAL, DEFAULT_PING_TIMEOUT};

#[cfg(any(test, feature = "testing"))]
pub type Bytes = Vec<u8>;
#[cfg(not(any(test, feature = "testing")))]
pub(crate) type Bytes = Vec<u8>;

// TODO(Shahak): add peer manager config to the network config
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
/// use apollo_network::{MultiaddrVectorConfig, NetworkConfig};
/// use starknet_api::core::ChainId;
///
/// let bootstrap_peer_id = "12D3KooWQYHvEJzuBPEXdwMfVdPGXeEFSioa7YcXqWn5Ey6qM8q7".parse().unwrap();
/// let config = NetworkConfig {
///     port: 10000,
///     chain_id: ChainId::Mainnet,
///     bootstrap_peer_multiaddr: Some(MultiaddrVectorConfig {
///         domain: vec!["1.2.3.4".to_string()],
///         port: vec![10000],
///         peer_id: vec![bootstrap_peer_id],
///     }),
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
#[validate(schema(function = "validate_advertised_multiaddr_peer_id"))]
pub struct NetworkConfig {
    /// TCP port for incoming connections. Default: 10000
    pub port: u16,

    /// Maximum session duration before timeout. Applies to inbound and outbound SQMR sessions.
    /// Default: 120 seconds
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub session_timeout: Duration,

    /// Maximum idle time before closing a connection. Default: 120 seconds
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub idle_connection_timeout: Duration,

    /// Bootstrap peer multiaddresses for initial connectivity. Each must include a valid peer ID.
    /// Format: `/ip4/1.2.3.4/tcp/10000/p2p/<peer-id>`. Default: None
    #[validate(nested)]
    pub bootstrap_peer_multiaddr: Option<MultiaddrVectorConfig>,

    /// Optional 32-byte Ed25519 private key for deterministic peer ID generation.
    /// If None, a random key is generated on each startup. Default: None
    #[validate(custom(function = "validate_optional_sensitive_vec_u256"))]
    #[serde(deserialize_with = "deserialize_optional_sensitive_vec_u8")]
    pub secret_key: Option<Sensitive<Vec<u8>>>,

    /// Optional external multiaddress advertised to other peers. Useful for NAT traversal.
    /// Default: None (automatic detection)
    #[validate(nested)]
    pub advertised_multiaddr: Option<MultiaddrConfig>,

    /// Starknet chain ID. Ensures connections only to peers on the same network.
    /// Default: ChainId::Mainnet
    pub chain_id: ChainId,

    /// Configuration for peer discovery mechanisms (Kademlia DHT, heartbeat intervals, etc.)
    pub discovery_config: DiscoveryConfig,

    /// Configuration for peer lifecycle and reputation management
    pub peer_manager_config: PeerManagerConfig,

    /// Buffer size for broadcasted message metadata. Default: 100000
    pub broadcasted_message_metadata_buffer_size: usize,

    /// Buffer size for reported peer IDs (peers flagged for malicious behavior). Default: 100000
    pub reported_peer_ids_buffer_size: usize,
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub prune_dead_connections_ping_interval: Duration,
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub prune_dead_connections_ping_timeout: Duration,
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
            ser_param(
                "prune_dead_connections_ping_interval",
                &self.prune_dead_connections_ping_interval.as_secs(),
                "The interval in seconds between each prune dead connections ping check.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "prune_dead_connections_ping_timeout",
                &self.prune_dead_connections_ping_timeout.as_secs(),
                "The timeout in seconds for a ping to be considered failed.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "secret_key",
                &serialize_optional_vec_u8(
                    &self.secret_key.as_ref().map(|s| s.clone().expose_secret()),
                ),
                "The secret key used for building the peer id. If it's an empty string a random one \
                 will be used.",
                ParamPrivacyInput::Private,
            ),
        ]);
        config.extend(ser_optional_sub_config(
            &self.bootstrap_peer_multiaddr,
            "bootstrap_peer_multiaddr",
        ));
        config.extend(prepend_sub_config_name(self.discovery_config.dump(), "discovery_config"));
        config.extend(prepend_sub_config_name(
            self.peer_manager_config.dump(),
            "peer_manager_config",
        ));
        config.extend(ser_optional_sub_config(&self.advertised_multiaddr, "advertised_multiaddr"));

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
            prune_dead_connections_ping_interval: DEFAULT_PING_INTERVAL,
            prune_dead_connections_ping_timeout: DEFAULT_PING_TIMEOUT,
        }
    }
}

/// Returns the libp2p protocol prefix for the given domain string:
/// `ip4` for IPv4 addresses, `ip6` for IPv6 addresses, and `dns` for everything else.
fn domain_protocol(domain: &str) -> &'static str {
    if domain.parse::<std::net::Ipv4Addr>().is_ok() {
        "ip4"
    } else if domain.parse::<std::net::Ipv6Addr>().is_ok() {
        "ip6"
    } else {
        "dns"
    }
}

/// A subconfig used to define a single multiaddr.
/// The domain is interpreted as an IPv4 address, IPv6 address, or DNS name, producing
/// /ip4/{domain}/tcp/{port}, /ip6/{domain}/tcp/{port}, or /dns/{domain}/tcp/{port} respectively.
/// The /p2p/{peer_id} component is appended when peer_id is Some.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Validate)]
#[validate(schema(function = "validate_multiaddr_config"))]
pub struct MultiaddrConfig {
    pub domain: String,
    pub port: u16,
    pub peer_id: Option<PeerId>,
}

impl TryFrom<MultiaddrConfig> for Multiaddr {
    type Error = ValidationError;
    fn try_from(config: MultiaddrConfig) -> Result<Multiaddr, ValidationError> {
        let base =
            format!("/{}/{}/tcp/{}", domain_protocol(&config.domain), config.domain, config.port);
        let addr_str = match config.peer_id {
            Some(peer_id) => format!("{}/p2p/{}", base, peer_id),
            None => base,
        };
        addr_str.parse::<Multiaddr>().map_err(|e| {
            ValidationError::new("Failed to parse multiaddr").with_message(e.to_string().into())
        })
    }
}

impl SerializeConfig for MultiaddrConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = BTreeMap::from_iter([
            ser_param("domain", &self.domain, "The domain to use.", ParamPrivacyInput::Public),
            ser_param("port", &self.port, "The port to use.", ParamPrivacyInput::Public),
        ]);
        config.extend(ser_optional_param(
            &self.peer_id,
            PeerId::from_multihash(Multihash::default()).unwrap(),
            "peer_id",
            "The peer id to use. If not provided, the multiaddr will not contain a peer id.",
            ParamPrivacyInput::Public,
        ));
        config
    }
}

fn validate_multiaddr_config(config: &MultiaddrConfig) -> Result<(), ValidationError> {
    if config.domain.is_empty() {
        return Err(ValidationError::new("Domain must not be empty."));
    }
    let _: Multiaddr = config.clone().try_into()?;
    Ok(())
}

/// A subconfig used to define a vector of multiaddrs.
/// Each domain is interpreted as an IPv4 address, IPv6 address, or DNS name, producing
/// /ip4/{domain}/tcp/{port}/p2p/{peer_id}, /ip6/{domain}/tcp/{port}/p2p/{peer_id}, or
/// /dns/{domain}/tcp/{port}/p2p/{peer_id} respectively.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Validate, Default)]
#[validate(schema(function = "validate_multiaddr_vector_config"))]
pub struct MultiaddrVectorConfig {
    #[serde(deserialize_with = "deserialize_vec")]
    pub domain: Vec<String>,
    #[serde(deserialize_with = "deserialize_vec")]
    pub port: Vec<u16>,
    #[serde(deserialize_with = "deserialize_vec")]
    pub peer_id: Vec<PeerId>,
}

impl TryFrom<MultiaddrVectorConfig> for Vec<Multiaddr> {
    type Error = ValidationError;
    fn try_from(config: MultiaddrVectorConfig) -> Result<Vec<Multiaddr>, ValidationError> {
        if config.domain.len() != config.port.len() || config.domain.len() != config.peer_id.len() {
            return Err(ValidationError::new(
                "Domain, port and peer id must have the same length.",
            ));
        }
        config
            .domain
            .iter()
            .zip(config.port.iter())
            .zip(config.peer_id.iter())
            .map(|((domain, port), peer_id)| {
                format!("/{}/{}/tcp/{}/p2p/{}", domain_protocol(domain), domain, port, peer_id)
                    .parse::<Multiaddr>()
                    .map_err(|e| {
                        ValidationError::new("Failed to parse multiaddr")
                            .with_message(e.to_string().into())
                    })
            })
            .collect()
    }
}

impl TryFrom<Vec<Multiaddr>> for MultiaddrVectorConfig {
    type Error = ValidationError;
    fn try_from(addrs: Vec<Multiaddr>) -> Result<MultiaddrVectorConfig, ValidationError> {
        let mut domain = Vec::with_capacity(addrs.len());
        let mut port = Vec::with_capacity(addrs.len());
        let mut peer_id = Vec::with_capacity(addrs.len());
        for addr in addrs {
            let mut addr_domain = None;
            let mut addr_port = None;
            let mut addr_peer_id = None;
            for protocol in addr.iter() {
                match protocol {
                    libp2p::multiaddr::Protocol::Ip4(ip) => addr_domain = Some(ip.to_string()),
                    libp2p::multiaddr::Protocol::Ip6(ip) => addr_domain = Some(ip.to_string()),
                    libp2p::multiaddr::Protocol::Dns(name)
                    | libp2p::multiaddr::Protocol::Dns4(name)
                    | libp2p::multiaddr::Protocol::Dns6(name) => {
                        addr_domain = Some(name.to_string())
                    }
                    libp2p::multiaddr::Protocol::Tcp(p) => addr_port = Some(p),
                    libp2p::multiaddr::Protocol::P2p(id) => addr_peer_id = Some(id),
                    _ => {}
                }
            }
            domain.push(
                addr_domain
                    .ok_or_else(|| ValidationError::new("Multiaddr missing domain component"))?,
            );
            port.push(
                addr_port
                    .ok_or_else(|| ValidationError::new("Multiaddr missing TCP port component"))?,
            );
            peer_id.push(
                addr_peer_id.ok_or_else(|| {
                    ValidationError::new("Multiaddr missing p2p peer ID component")
                })?,
            );
        }
        Ok(MultiaddrVectorConfig { domain, port, peer_id })
    }
}

impl SerializeConfig for MultiaddrVectorConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "domain",
                &serialize_slice(&self.domain),
                "The domain to use.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "port",
                &serialize_slice(&self.port),
                "The port to use.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "peer_id",
                &serialize_slice(&self.peer_id),
                "The peer id to use.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

fn validate_multiaddr_vector_config(config: &MultiaddrVectorConfig) -> Result<(), ValidationError> {
    if config.domain.len() != config.port.len() || config.domain.len() != config.peer_id.len() {
        return Err(ValidationError::new("Domain, port and peer id must have the same length."));
    }
    if config.domain.iter().any(|d| d.is_empty()) {
        return Err(ValidationError::new("Domain must not be empty."));
    }
    let _: Vec<Multiaddr> = config.clone().try_into()?;
    let unique_peer_ids: HashSet<_> = config.peer_id.iter().collect();
    if unique_peer_ids.len() != config.peer_id.len() {
        return Err(ValidationError::new("Bootstrap peer PeerIds are not unique."));
    }
    Ok(())
}

/// Validates that if advertised_multiaddr contains a peer id, it matches the peer id
/// generated from secret_key.
///
/// If advertised_multiaddr contains a peer id, secret_key must not be None.
fn validate_advertised_multiaddr_peer_id(
    config: &NetworkConfig,
) -> Result<(), validator::ValidationError> {
    let Some(advertised_multiaddr) = &config.advertised_multiaddr else {
        return Ok(());
    };

    let Some(advertised_peer_id) = advertised_multiaddr.peer_id else {
        // If advertised_multiaddr doesn't contain a peer id, no validation needed
        return Ok(());
    };

    // If advertised_multiaddr contains a peer id, secret_key must not be None
    let Some(secret_key) = &config.secret_key else {
        return Err(ValidationError::new(
            "If advertised_multiaddr contains a peer id, secret_key must be provided.",
        ));
    };

    // Generate the peer id from secret_key
    let keypair =
        Keypair::ed25519_from_bytes(secret_key.clone().expose_secret()).map_err(|err| {
            let mut error = ValidationError::new("Failed to parse secret_key as Ed25519 keypair.");
            error.message = Some(std::borrow::Cow::from(format!("Error: {err}")));
            error
        })?;
    let my_peer_id = keypair.public().to_peer_id();

    if advertised_peer_id != my_peer_id {
        let mut error = ValidationError::new(
            "The peer id in advertised_multiaddr does not match the peer id generated from \
             secret_key.",
        );
        error.message = Some(std::borrow::Cow::from(format!(
            "advertised peer id: {advertised_peer_id}, my peer id: {my_peer_id}"
        )));
        return Err(error);
    }

    Ok(())
}
