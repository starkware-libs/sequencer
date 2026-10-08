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
#[cfg(test)]
mod testing_utils;

pub use apollo_network_config::discovery::{DiscoveryConfig, RetryConfig};
use behaviours::bootstrapping::BootstrappingBehaviour;
use behaviours::dialing::DialingBehaviour;
use behaviours::kad_requesting::KadRequestingBehaviour;
use libp2p::swarm::NetworkBehaviour;
use libp2p::{Multiaddr, PeerId};

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
