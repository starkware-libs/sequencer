use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use futures::Stream;
use libp2p::swarm::behaviour::ConnectionEstablished;
use libp2p::swarm::dial_opts::{DialOpts, PeerCondition};
use libp2p::swarm::{
    dummy,
    AddressChange,
    ConnectionClosed,
    ConnectionHandler,
    DialFailure,
    FromSwarm,
    ToSwarm,
};
use libp2p::{Multiaddr, PeerId};
use tokio::time::{Instant, Sleep};
use tokio_retry::strategy::ExponentialBackoff;
use tracing::info;

use crate::discovery::{RetryConfig, ToOtherBehaviourEvent};

const NUMBER_OF_CONNECTIONS: usize = 20;

/// A stream that handles the bootstrapping with a bootstrap peer.
/// This stream will automatically dial the bootstrap peer to establish and maintain
/// NUMBER_OF_CONNECTIONS connections.
pub struct BootstrapPeerEventStream {
    dial_retry_config: RetryConfig,
    peer_address: Multiaddr,
    peer_id: PeerId,
    established_connections: usize,
    pending_dials: usize,
    should_add_peer_to_kad_routing_table: bool,
    dial_retry_strategy: ExponentialBackoff,
    time_for_next_bootstrap_dial: Instant,
    waker: Option<Waker>,
    sleeper: Option<Pin<Box<Sleep>>>,
}

impl BootstrapPeerEventStream {
    fn wake_if_needed(&mut self) {
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }

    pub fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
        let now = tokio::time::Instant::now();
        match event {
            FromSwarm::DialFailure(DialFailure { peer_id: Some(peer_id), .. })
                if peer_id == self.peer_id =>
            {
                if self.pending_dials > 0 {
                    self.pending_dials -= 1;
                } else {
                    // Not my dial
                    return;
                }
                // For the case that the reason for failure is consistent (e.g the bootstrap peer
                // is down), we sleep before redialing
                let delta_duration = self
                    .dial_retry_strategy
                    .next()
                    .expect("Dial sleep strategy ended even though it's an infinite iterator.");
                self.time_for_next_bootstrap_dial = now + delta_duration;
                self.wake_if_needed();
            }
            FromSwarm::ConnectionEstablished(ConnectionEstablished { peer_id, .. })
                if peer_id == self.peer_id =>
            {
                self.established_connections += 1;
                if self.pending_dials > 0 {
                    self.pending_dials -= 1;
                }
                // Reset retry dial strategy to original values since we succeeded in dialing
                self.dial_retry_strategy = self.dial_retry_config.strategy();
                self.wake_if_needed();
            }
            FromSwarm::ConnectionClosed(ConnectionClosed { peer_id, .. })
                if peer_id == self.peer_id =>
            {
                if self.established_connections > 0 {
                    self.established_connections -= 1;
                }
                if self.established_connections == 0 {
                    self.should_add_peer_to_kad_routing_table = true;
                }
                self.time_for_next_bootstrap_dial = now;
                self.wake_if_needed();
            }
            FromSwarm::AddressChange(AddressChange { peer_id, .. }) if peer_id == self.peer_id => {
                todo!();
            }
            _ => {}
        }
    }

    pub fn new(
        bootstrap_dial_retry_config: RetryConfig,
        bootstrap_peer_id: PeerId,
        bootstrap_peer_address: Multiaddr,
    ) -> Self {
        let bootstrap_dial_retry_strategy = bootstrap_dial_retry_config.strategy();
        Self {
            dial_retry_config: bootstrap_dial_retry_config,
            peer_id: bootstrap_peer_id,
            peer_address: bootstrap_peer_address,
            established_connections: 0,
            pending_dials: 0,
            should_add_peer_to_kad_routing_table: true,
            dial_retry_strategy: bootstrap_dial_retry_strategy,
            time_for_next_bootstrap_dial: tokio::time::Instant::now(),
            waker: None,
            sleeper: None,
        }
    }

    fn initiate_dial<T, W>(&mut self) -> ToSwarm<T, W> {
        self.sleeper = None;
        self.pending_dials += 1;
        info!(?self.peer_id, ?self.peer_address, "Performing bootstrap dial");
        ToSwarm::Dial {
            opts: DialOpts::peer_id(self.peer_id)
                    .addresses(vec![self.peer_address.clone()])
                    // Allow dialing even if already connected, to establish multiple connections.
                    // But avoid dialing if we're already in the process of dialing.
                    .condition(PeerCondition::NotDialing)
                    .build(),
        }
    }
}

impl Stream for BootstrapPeerEventStream {
    type Item = ToSwarm<
        ToOtherBehaviourEvent,
        <dummy::ConnectionHandler as ConnectionHandler>::FromBehaviour,
    >;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let now = tokio::time::Instant::now();
        // The future contract requires that we always awake the most recent waker.
        self.waker = Some(cx.waker().clone());

        // If we have at least one connection and need to add the peer to the routing table, do that
        // first.
        if self.established_connections > 0 && self.should_add_peer_to_kad_routing_table {
            self.should_add_peer_to_kad_routing_table = false;
            return Poll::Ready(Some(ToSwarm::GenerateEvent(
                ToOtherBehaviourEvent::FoundListenAddresses {
                    peer_id: self.peer_id,
                    listen_addresses: vec![self.peer_address.clone()],
                },
            )));
        }

        // Check if we need to establish more connections.
        let total_connections = self.established_connections + self.pending_dials;
        if total_connections < NUMBER_OF_CONNECTIONS {
            // We need more connections. Check if it's time to dial.
            if self.time_for_next_bootstrap_dial <= now {
                return Poll::Ready(Some(self.initiate_dial()));
            }

            // Not time to dial yet. Set up or poll the sleeper.
            if self.sleeper.is_none() {
                let next_wake_up = self.time_for_next_bootstrap_dial;
                self.sleeper = Some(Box::pin(tokio::time::sleep_until(next_wake_up)));
            }
            let sleeper =
                self.sleeper.as_mut().expect("Sleeper cannot be None after being created above.");

            match sleeper.as_mut().poll(cx) {
                Poll::Ready(()) => {
                    info!(
                        "Sleeper completed sleep in the time between checking it's not time to \
                         dial yet, and polling the sleeper. This should be extremely rare/non \
                         existent"
                    );
                    Poll::Ready(Some(self.initiate_dial()))
                }
                Poll::Pending => Poll::Pending,
            }
        } else {
            // We have enough connections (established + pending). Nothing to do.
            Poll::Pending
        }
    }
}
