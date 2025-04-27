use std::pin::Pin;
use std::task::{Context, Poll};

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
use tokio::time::Instant;
use tokio_retry::strategy::ExponentialBackoff;
use tracing::debug;

use crate::discovery::behaviours::TimeWakerManager;
use crate::discovery::{RetryConfig, ToOtherBehaviourEvent};

pub struct BootstrapPeer {
    bootstrap_dial_retry_config: RetryConfig,
    bootstrap_peer_address: Multiaddr,
    bootstrap_peer_id: PeerId,
    is_dialing_to_bootstrap_peer: bool,
    is_connected_to_bootstrap_peer: bool,
    is_bootstrap_in_kad_routing_table: bool,
    bootstrap_dial_retry_strategy: ExponentialBackoff,
    time_for_next_bootstrap_dial: Instant,
    waker_manager: TimeWakerManager,
}

impl BootstrapPeer {
    pub fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
        let now = tokio::time::Instant::now();
        match event {
            FromSwarm::DialFailure(DialFailure { peer_id: Some(peer_id), .. })
                if peer_id == self.bootstrap_peer_id =>
            {
                self.is_dialing_to_bootstrap_peer = false;
                // For the case that the reason for failure is consistent (e.g the bootstrap peer
                // is down), we sleep before redialing
                let delta_duration = self
                    .bootstrap_dial_retry_strategy
                    .next()
                    .expect("Dial sleep strategy ended even though it's an infinite iterator.");
                self.time_for_next_bootstrap_dial = now + delta_duration;
                self.waker_manager.wake();
            }
            FromSwarm::ConnectionEstablished(ConnectionEstablished { peer_id, .. })
                if peer_id == self.bootstrap_peer_id =>
            {
                self.is_connected_to_bootstrap_peer = true;
                self.is_dialing_to_bootstrap_peer = false;
                // recreating the strategy since we've succeeded in the dial
                self.bootstrap_dial_retry_strategy = self.bootstrap_dial_retry_config.strategy();
                self.waker_manager.wake();
            }
            FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                remaining_established,
                ..
            }) if peer_id == self.bootstrap_peer_id && remaining_established == 0 => {
                self.is_connected_to_bootstrap_peer = false;
                self.is_dialing_to_bootstrap_peer = false;
                self.is_bootstrap_in_kad_routing_table = false;
                self.time_for_next_bootstrap_dial = now;
                self.waker_manager.wake();
            }
            FromSwarm::AddressChange(AddressChange { peer_id, old, new, .. })
                if peer_id == self.bootstrap_peer_id =>
            {
                debug!("Address of bootstrap peer {} changed from {:?} to {:?}", peer_id, old, new);
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
            bootstrap_dial_retry_config,
            bootstrap_peer_id,
            bootstrap_peer_address,
            is_dialing_to_bootstrap_peer: false,
            is_connected_to_bootstrap_peer: false,
            is_bootstrap_in_kad_routing_table: false,
            bootstrap_dial_retry_strategy,
            time_for_next_bootstrap_dial: tokio::time::Instant::now(),
            waker_manager: Default::default(),
        }
    }
}

impl Stream for BootstrapPeer {
    type Item = ToSwarm<
        ToOtherBehaviourEvent,
        <dummy::ConnectionHandler as ConnectionHandler>::FromBehaviour,
    >;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let now = tokio::time::Instant::now();
        self.waker_manager.add_waker(cx.waker());

        if self.is_connected_to_bootstrap_peer && !self.is_bootstrap_in_kad_routing_table {
            self.is_bootstrap_in_kad_routing_table = true;
            return Poll::Ready(Some(ToSwarm::GenerateEvent(
                ToOtherBehaviourEvent::FoundListenAddresses {
                    peer_id: self.bootstrap_peer_id,
                    listen_addresses: vec![self.bootstrap_peer_address.clone()],
                },
            )));
        }

        let should_dial =
            !(self.is_dialing_to_bootstrap_peer) && !(self.is_connected_to_bootstrap_peer);

        if should_dial && (self.time_for_next_bootstrap_dial <= now) {
            self.is_dialing_to_bootstrap_peer = true;
            return Poll::Ready(Some(ToSwarm::Dial {
                opts: DialOpts::peer_id(self.bootstrap_peer_id)
                        .addresses(vec![self.bootstrap_peer_address.clone()])
                        // The peer manager might also be dialing to the bootstrap node.
                        .condition(PeerCondition::DisconnectedAndNotDialing)
                        .build(),
            }));
        }

        if should_dial {
            let next_wake_up = self.time_for_next_bootstrap_dial;
            let _ = self.waker_manager.wake_at(cx, next_wake_up);
        }
        Poll::Pending
    }
}
