use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, Instant};

use futures::FutureExt;
use libp2p::swarm::behaviour::ConnectionEstablished;
use libp2p::swarm::dial_opts::{DialOpts, PeerCondition};
use libp2p::swarm::{AddressChange, ConnectionClosed, DialFailure, FromSwarm, ToSwarm};
use libp2p::{Multiaddr, PeerId};
use tokio_retry::strategy::ExponentialBackoff;

use super::{PollFutureOutput, RetryConfig, ToOtherBehaviourEvent};

pub struct BootstrapPeerMetadata {
    pub bootstrap_peer_address: Multiaddr,
    pub bootstrap_peer_id: PeerId,
    is_dialing_to_bootstrap_peer: bool,
    is_connected_to_bootstrap_peer: bool,
    is_bootstrap_in_kad_routing_table: bool,
    bootstrap_dial_retry_strategy: ExponentialBackoff,
    time_for_next_bootstrap_dial: Instant,
    time_for_next_kad_query: Instant,
}

impl BootstrapPeerMetadata {
    pub fn new(
        bootstrap_peer_id: PeerId,
        bootstrap_peer_address: Multiaddr,
        bootstrap_dial_retry_strategy: ExponentialBackoff,
    ) -> Self {
        Self {
            bootstrap_peer_id,
            bootstrap_peer_address,
            is_dialing_to_bootstrap_peer: false,
            is_connected_to_bootstrap_peer: false,
            is_bootstrap_in_kad_routing_table: false,
            bootstrap_dial_retry_strategy,
            time_for_next_bootstrap_dial: Instant::now(),
            time_for_next_kad_query: Instant::now(),
        }
    }

    pub fn on_swarm_event(
        &mut self,
        event: &FromSwarm<'_>,
        bootstrap_dial_retry_config: &RetryConfig,
    ) {
        match event {
            FromSwarm::DialFailure(DialFailure { peer_id: Some(peer_id), .. })
                if *peer_id == self.bootstrap_peer_id =>
            {
                self.is_dialing_to_bootstrap_peer = false;
                // For the case that the reason for failure is consistent (e.g the bootstrap peer
                // is down), we sleep before redialing
                self.time_for_next_bootstrap_dial = Instant::now()
                    + self
                        .bootstrap_dial_retry_strategy
                        .next()
                        .expect("Dial sleep strategy ended even though it's an infinite iterator.");
            }
            FromSwarm::ConnectionEstablished(ConnectionEstablished { peer_id, .. })
                if *peer_id == self.bootstrap_peer_id =>
            {
                self.is_connected_to_bootstrap_peer = true;
                self.is_dialing_to_bootstrap_peer = false;
                self.bootstrap_dial_retry_strategy = bootstrap_dial_retry_config.strategy();
            }
            FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                remaining_established,
                ..
            }) if *peer_id == self.bootstrap_peer_id && *remaining_established == 0 => {
                self.is_connected_to_bootstrap_peer = false;
                self.is_dialing_to_bootstrap_peer = false;
                self.is_bootstrap_in_kad_routing_table = false;
            }
            FromSwarm::AddressChange(AddressChange { peer_id, .. })
                if *peer_id == self.bootstrap_peer_id =>
            {
                todo!();
            }
            _ => {}
        }
    }

    pub(super) fn poll(
        &self,
        heartbeat_interval: Duration,
    ) -> Pin<Box<dyn Future<Output = PollFutureOutput> + Send>> {
        let is_connected_to_bootstrap_peer = self.is_connected_to_bootstrap_peer;
        let is_bootstrap_in_kad_routing_table = self.is_bootstrap_in_kad_routing_table;
        let bootstrap_peer_id = self.bootstrap_peer_id;
        let bootstrap_peer_address = self.bootstrap_peer_address.clone();
        let time_for_next_bootstrap_dial = self.time_for_next_bootstrap_dial;
        let time_for_next_kad_query = self.time_for_next_kad_query;
        let is_dialing_to_bootstrap_peer = self.is_dialing_to_bootstrap_peer;
        async move {
            if is_connected_to_bootstrap_peer && !is_bootstrap_in_kad_routing_table {
                return PollFutureOutput {
                    event: ToSwarm::GenerateEvent(ToOtherBehaviourEvent::FoundListenAddresses {
                        peer_id: bootstrap_peer_id,
                        listen_addresses: vec![bootstrap_peer_address],
                    }),
                    is_bootstrap_in_kad_routing_table: Some(true),
                    is_dialing_to_bootstrap_peer: None,
                    time_for_next_kad_query: None,
                };
            }

            // TODO(Shahak): If one of the last two conditions is false, register a waker and
            // wake it when we receive an event that we've disconnected from the bootstrap peer.
            // (Right now, when we're disconnected from the bootstrap peer, we'll wait for next
            // kad query even if time_for_next_bootstrap_dial is smaller than
            // time_for_next_kad_query)
            if time_for_next_bootstrap_dial < time_for_next_kad_query
                    // No need to perform a dial if there's an active dial attempt or we're already
                    // connected.
                    && !(is_dialing_to_bootstrap_peer)
                    && !(is_connected_to_bootstrap_peer)
            {
                tokio::time::sleep_until(time_for_next_bootstrap_dial.into()).await;
                PollFutureOutput {
                    event: ToSwarm::Dial {
                        opts: DialOpts::peer_id(bootstrap_peer_id)
                                .addresses(vec![bootstrap_peer_address])
                                // The peer manager might also be dialing to the bootstrap node.
                                .condition(PeerCondition::DisconnectedAndNotDialing)
                                .build(),
                    },
                    is_dialing_to_bootstrap_peer: Some(true),
                    is_bootstrap_in_kad_routing_table: None,
                    time_for_next_kad_query: None,
                }
            } else {
                tokio::time::sleep_until(time_for_next_kad_query.into()).await;
                PollFutureOutput {
                    event: ToSwarm::GenerateEvent(ToOtherBehaviourEvent::RequestKadQuery(
                        libp2p::identity::PeerId::random(),
                    )),
                    time_for_next_kad_query: Some(Instant::now() + heartbeat_interval),
                    is_bootstrap_in_kad_routing_table: None,
                    is_dialing_to_bootstrap_peer: None,
                }
            }
        }
        .boxed()
    }

    pub fn update_after_poll(
        &mut self,
        is_bootstrap_in_kad_routing_table: Option<bool>,
        is_dialing_to_bootstrap_peer: Option<bool>,
        time_for_next_kad_query: Option<Instant>,
    ) {
        self.is_bootstrap_in_kad_routing_table =
            is_bootstrap_in_kad_routing_table.unwrap_or(self.is_bootstrap_in_kad_routing_table);
        self.is_dialing_to_bootstrap_peer =
            is_dialing_to_bootstrap_peer.unwrap_or(self.is_dialing_to_bootstrap_peer);
        self.time_for_next_kad_query =
            time_for_next_kad_query.unwrap_or(self.time_for_next_kad_query);
    }
}
