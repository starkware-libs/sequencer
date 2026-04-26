//! Peer discovery and network bootstrapping functionality.
//!
//! This module implements peer discovery mechanisms that enable nodes to find and
//! connect to other peers in the Starknet network. It combines bootstrapping with
//! initial known peers and ongoing peer discovery through Kademlia DHT queries.
//!
//! ## Key Components
//!
//! - **Bootstrapping**: Initial connection to known bootstrap peers
//! - **Kademlia Queries**: Ongoing peer discovery through DHT queries
//! - **Identify Protocol**: Peer capability and address discovery
//! - **Retry Logic**: Exponential backoff for failed connection attempts
//!
//! ## Discovery Process
//!
//! 1. **Bootstrap Phase**: Connect to configured bootstrap peers
//! 2. **DHT Integration**: Join the Kademlia DHT network
//! 3. **Peer Discovery**: Continuously discover new peers through DHT queries
//! 4. **Address Resolution**: Resolve and validate peer addresses
//!
//! The discovery process is designed to be resilient to network partitions and
//! node failures, ensuring robust connectivity across the network.

mod behaviours;
#[cfg(test)]
mod discovery_test;
pub mod identify_impl;
pub mod kad_impl;

use std::collections::BTreeMap;
use std::time::Duration;

use apollo_config::converters::{
    deserialize_milliseconds_to_duration,
    deserialize_seconds_to_duration,
};
use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use behaviours::bootstrapping::BootstrappingBehaviour;
use behaviours::dialing::DialingBehaviour;
use behaviours::kad_requesting::KadRequestingBehaviour;
use libp2p::swarm::NetworkBehaviour;
use libp2p::{Multiaddr, PeerId};
use serde::{Deserialize, Serialize};
use tokio_retry::strategy::ExponentialBackoff;

use crate::mixed_behaviour;
use crate::mixed_behaviour::BridgedBehaviour;

/// Events emitted by the discovery behavior to coordinate with other network behaviors.
///
/// The discovery behavior doesn't emit external events directly but instead
/// coordinates with other behaviors (like Kademlia) to implement the full
/// discovery process.
#[derive(Debug)]
pub enum ToOtherBehaviourEvent {
    /// Request a Kademlia query for the specified peer.
    ///
    /// This event is used to trigger Kademlia DHT queries to find peers
    /// or gather routing table information.
    RequestKadQuery(PeerId),

    /// Discovered listen addresses for a peer.
    ///
    /// This event is emitted when the discovery process finds new listening
    /// addresses for a known peer, typically through the identify protocol
    /// or DHT queries.
    FoundListenAddresses {
        /// The peer whose addresses were discovered.
        peer_id: PeerId,
        /// The discovered listening addresses.
        listen_addresses: Vec<Multiaddr>,
    },

    /// Request dialing a peer at the given addresses.
    RequestDial { peer_id: PeerId, addresses: Vec<Multiaddr> },
}

/// Main discovery behavior that orchestrates peer discovery mechanisms.
///
/// This behavior combines bootstrapping and Kademlia requesting to provide
/// a comprehensive peer discovery system. It handles:
///
/// - Initial bootstrapping with configured peers
/// - Periodic Kademlia queries for ongoing peer discovery
/// - Address resolution and validation
/// - Retry logic for failed connections
///
/// The behavior operates continuously in the background, maintaining
/// network connectivity and discovering new peers as needed.
#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "ToOtherBehaviourEvent")]
pub struct Behaviour {
    /// Handles initial bootstrapping with configured peers.
    boot_strapping: BootstrappingBehaviour,
    /// Manages ongoing Kademlia queries for peer discovery.
    kad_requesting: KadRequestingBehaviour,
    /// Manages dialing to peers with retries.
    dialing: DialingBehaviour,
}

/// Configuration for the peer discovery system.
///
/// This struct contains all parameters needed to configure the discovery
/// behavior, including retry policies and timing intervals.
///
/// # Examples
///
/// ```rust
/// use std::time::Duration;
///
/// use apollo_network::discovery::{DiscoveryConfig, RetryConfig};
///
/// let config = DiscoveryConfig {
///     bootstrap_dial_retry_config: RetryConfig {
///         base_delay_millis: 100,
///         max_delay_seconds: Duration::from_secs(10),
///         factor: 2,
///         new_connection_stabilization_millis: Duration::from_millis(2000),
///     },
///     heartbeat_interval: Duration::from_millis(500),
/// };
/// ```
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DiscoveryConfig {
    /// Configuration for retrying failed bootstrap peer connections.
    pub bootstrap_dial_retry_config: RetryConfig,

    /// Interval between periodic discovery operations.
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    pub heartbeat_interval: Duration,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            bootstrap_dial_retry_config: RetryConfig::default(),
            heartbeat_interval: Duration::from_millis(100),
        }
    }
}

impl SerializeConfig for DiscoveryConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = BTreeMap::from([ser_param(
            "heartbeat_interval",
            &self.heartbeat_interval.as_millis(),
            "The interval between each discovery (Kademlia) query in milliseconds.",
            ParamPrivacyInput::Public,
        )]);
        dump.append(&mut prepend_sub_config_name(
            self.bootstrap_dial_retry_config.dump(),
            "bootstrap_dial_retry_config",
        ));
        dump
    }
}

/// Configuration for exponential backoff retry logic.
///
/// This struct defines the parameters for the exponential backoff strategy
/// used when retrying failed operations, particularly bootstrap peer connections.
///
/// # Exponential Backoff Algorithm
///
/// The delay between retry attempts follows this pattern:
/// - 1st retry: `base_delay_millis**1 * factor`
/// - 2nd retry: `base_delay_millis**2 * factor`
/// - 3rd retry: `base_delay_millis**3 * factor`
/// - And so on, capped at `max_delay_seconds`
///
/// # Examples
///
/// ```rust
/// use std::time::Duration;
///
/// use apollo_network::discovery::RetryConfig;
///
/// // Aggressive retry (fast but more network usage)
/// let aggressive = RetryConfig {
///     base_delay_millis: 2,                          // double each time
///     max_delay_seconds: Duration::from_millis(100), // Cap at 0.1 seconds
///     factor: 7,                                     // start with 7ms
///     new_connection_stabilization_millis: Duration::from_millis(2000),
/// };
///
/// let mut strategy = aggressive.strategy();
/// assert_eq!(strategy.next(), Some(Duration::from_millis(14)));
/// assert_eq!(strategy.next(), Some(Duration::from_millis(28)));
/// assert_eq!(strategy.next(), Some(Duration::from_millis(56)));
/// assert_eq!(strategy.next(), Some(Duration::from_millis(100)));
/// ```
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RetryConfig {
    /// Base of the exponential backoff in milliseconds, this will be the delay before the first
    /// retry (the first delay after the first attempt)
    pub base_delay_millis: u64,

    /// Maximum delay of the exponential backoff.
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub max_delay_seconds: Duration,

    /// Multiplication factor for the exponential backoff.
    pub factor: u64,

    /// Milliseconds to wait on a new connection before treating it as stable. Redials within
    /// this window (e.g. from an immediately refused connection) use accumulated backoff.
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    pub new_connection_stabilization_millis: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            base_delay_millis: 2,
            max_delay_seconds: Duration::from_secs(5),
            factor: 5,
            new_connection_stabilization_millis: Duration::from_millis(2000),
        }
    }
}

impl SerializeConfig for RetryConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([
            ser_param(
                "base_delay_millis",
                &self.base_delay_millis,
                "The base delay in milliseconds for the exponential backoff strategy.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_delay_seconds",
                &self.max_delay_seconds.as_secs(),
                "The maximum delay in seconds for the exponential backoff strategy.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "factor",
                &self.factor,
                "The factor for the exponential backoff strategy.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "new_connection_stabilization_millis",
                &self.new_connection_stabilization_millis.as_millis(),
                "Milliseconds to wait on a new connection before treating it as stable.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl RetryConfig {
    pub fn strategy(&self) -> ExponentialBackoff {
        ExponentialBackoff::from_millis(self.base_delay_millis)
            .max_delay(self.max_delay_seconds)
            .factor(self.factor)
    }
}

impl Behaviour {
    pub fn new(
        local_peer_id: PeerId,
        config: DiscoveryConfig,
        bootstrap_peers: Vec<(PeerId, Multiaddr)>,
    ) -> Self {
        Self {
            boot_strapping: BootstrappingBehaviour::new(local_peer_id, bootstrap_peers),
            kad_requesting: KadRequestingBehaviour::new(config.heartbeat_interval),
            // TODO(AndrewL): rename bootstrap_dial_retry_config to dial_retry_config since
            // it's now shared between bootstrap and general dialing behaviours.
            dialing: DialingBehaviour::new(config.bootstrap_dial_retry_config),
        }
    }

    pub fn set_target_peers(&mut self, peers: std::collections::HashSet<PeerId>) {
        let removed_peers = self.kad_requesting.set_target_peers(peers);
        for peer_id in &removed_peers {
            if !self.boot_strapping.is_bootstrap_peer(peer_id) {
                self.dialing.cancel_dial(peer_id);
            }
        }
    }
}

impl From<ToOtherBehaviourEvent> for mixed_behaviour::Event {
    fn from(event: ToOtherBehaviourEvent) -> Self {
        mixed_behaviour::Event::ToOtherBehaviourEvent(
            mixed_behaviour::ToOtherBehaviourEvent::Discovery(event),
        )
    }
}

impl BridgedBehaviour for Behaviour {
    fn on_other_behaviour_event(&mut self, event: &mixed_behaviour::ToOtherBehaviourEvent) {
        match event {
            mixed_behaviour::ToOtherBehaviourEvent::Kad(
                kad_impl::KadToOtherBehaviourEvent::FoundPeers(peers),
            ) => {
                self.kad_requesting.handle_kad_response(peers);
            }
            mixed_behaviour::ToOtherBehaviourEvent::Discovery(
                ToOtherBehaviourEvent::RequestDial { peer_id, addresses },
            ) => {
                self.dialing.request_dial(*peer_id, addresses.clone());
            }
            _ => {}
        }
    }
}
