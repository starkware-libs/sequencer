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
#[cfg(test)]
mod flow_test;
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
}

/// Discovery behaviour that handles the bootstrapping and Kademlia requesting
#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "ToOtherBehaviourEvent")]
pub struct Behaviour {
    boot_strapping: BootstrappingBehaviour,
    kad_requesting: KadRequestingBehaviour,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DiscoveryConfig {
    pub bootstrap_dial_retry_config: RetryConfig,
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
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self { base_delay_millis: 2, max_delay_seconds: Duration::from_secs(5), factor: 5 }
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
            boot_strapping: BootstrappingBehaviour::new(
                local_peer_id,
                config.bootstrap_dial_retry_config,
                bootstrap_peers,
            ),
            kad_requesting: KadRequestingBehaviour::new(config.heartbeat_interval),
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
    fn on_other_behaviour_event(&mut self, _event: &mixed_behaviour::ToOtherBehaviourEvent) {}
}
