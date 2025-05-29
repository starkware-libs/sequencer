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

/// A stream that handles the bootstrapping with a bootstrap peer.
/// This stream will automatically dial the bootstrap peer if not already connected.
///
/// This implementation assumes exclusivity of the bootstrap peer connection, meaning that
/// only one instance of this stream will be active for a given bootstrap peer at a time.
/// If multiple instances are created for the same bootstrap peer, they will interfere with each
/// other.
pub struct BootstrapPeerEventStream {
    dial_retry_config: RetryConfig,
    peer_address: Multiaddr,
    peer_id: PeerId,
    dial_mode: DialMode,
    should_add_peer_to_kad_routing_table: bool,
    dial_retry_strategy: ExponentialBackoff,
    time_for_next_bootstrap_dial: Instant,
    waker: Option<Waker>,
    sleeper: Option<Pin<Box<Sleep>>>,
}

enum DialMode {
    Dialing,
    Connected,
    Disconnected,
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
                self.dial_mode = DialMode::Disconnected;
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
                self.dial_mode = DialMode::Connected;
                // Reset retry dial strategy to original values since we succeeded in dialing
                self.dial_retry_strategy = self.dial_retry_config.strategy();
                self.wake_if_needed();
            }
            FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                remaining_established,
                ..
            }) if peer_id == self.peer_id && remaining_established == 0 => {
                self.dial_mode = DialMode::Disconnected;
                self.should_add_peer_to_kad_routing_table = true;
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
            dial_mode: DialMode::Disconnected,
            should_add_peer_to_kad_routing_table: true,
            dial_retry_strategy: bootstrap_dial_retry_strategy,
            time_for_next_bootstrap_dial: tokio::time::Instant::now(),
            waker: None,
            sleeper: None,
        }
    }

    fn switch_to_dialing_mode<T, W>(&mut self) -> ToSwarm<T, W> {
        self.sleeper = None;
        self.dial_mode = DialMode::Dialing;
        ToSwarm::Dial {
            opts: DialOpts::peer_id(self.peer_id)
                    .addresses(vec![self.peer_address.clone()])
                    // The peer manager might also be dialing to the bootstrap node.
                    .condition(PeerCondition::DisconnectedAndNotDialing)
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

        match self.dial_mode {
            DialMode::Connected => {
                if self.should_add_peer_to_kad_routing_table {
                    self.should_add_peer_to_kad_routing_table = false;
                    Poll::Ready(Some(ToSwarm::GenerateEvent(
                        ToOtherBehaviourEvent::FoundListenAddresses {
                            peer_id: self.peer_id,
                            listen_addresses: vec![self.peer_address.clone()],
                        },
                    )))
                } else {
                    // We are connected (and the peer is already in the routing table). Nothing for
                    // us to do until something changes.
                    Poll::Pending
                }
            }

            DialMode::Disconnected => {
                if self.time_for_next_bootstrap_dial <= now {
                    return Poll::Ready(Some(self.switch_to_dialing_mode()));
                }
                if self.sleeper.is_none() {
                    let next_wake_up = self.time_for_next_bootstrap_dial;
                    self.sleeper = Some(Box::pin(tokio::time::sleep_until(next_wake_up)));
                }
                let sleeper = self
                    .sleeper
                    .as_mut()
                    .expect("Sleeper cannot be None after being created above.");

                match sleeper.as_mut().poll(cx) {
                    Poll::Ready(()) => {
                        info!(
                            "Sleeper completed sleep in the time between checking it's not time \
                             to dial yet, and polling the sleeper. This should be extremely \
                             rare/non existent"
                        );
                        Poll::Ready(Some(self.switch_to_dialing_mode()))
                    }
                    Poll::Pending => Poll::Pending,
                }
            }
            DialMode::Dialing => Poll::Pending,
        }
    }
}
