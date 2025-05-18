use std::pin::Pin;
use std::task::{Context, Poll};

<<<<<<< HEAD
use futures::{FutureExt, Stream};
||||||| 0c5c7df2e
use libp2p::core::Endpoint;
=======
use futures::FutureExt;
use libp2p::core::Endpoint;
>>>>>>> origin/main
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

use crate::discovery::behaviours::{EventWakerManager, TimeWakerManager};
use crate::discovery::{RetryConfig, ToOtherBehaviourEvent};

/// A stream that handles the bootstrapping with a bootstrap peer.
/// This stream will automatically dial the bootstrap peer if not already connected.
pub struct BootstrapPeer {
    bootstrap_dial_retry_config: RetryConfig,
    bootstrap_peer_address: Multiaddr,
    bootstrap_peer_id: PeerId,
    is_dialing_to_bootstrap_peer: bool,
    is_connected_to_bootstrap_peer: bool,
    is_bootstrap_in_kad_routing_table: bool,
    bootstrap_dial_retry_strategy: ExponentialBackoff,
    time_for_next_bootstrap_dial: Instant,
    time_waker: TimeWakerManager,
    event_waker: EventWakerManager,
}

<<<<<<< HEAD
impl BootstrapPeer {
    pub fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
        let now = tokio::time::Instant::now();
||||||| 0c5c7df2e
impl NetworkBehaviour for BootstrappingBehaviour {
    type ConnectionHandler = dummy::ConnectionHandler;
    type ToSwarm = ToOtherBehaviourEvent;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        Ok(dummy::ConnectionHandler)
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        Ok(dummy::ConnectionHandler)
    }

    fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
=======
impl NetworkBehaviour for BootstrappingBehaviour {
    type ConnectionHandler = dummy::ConnectionHandler;
    type ToSwarm = ToOtherBehaviourEvent;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        Ok(dummy::ConnectionHandler)
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        Ok(dummy::ConnectionHandler)
    }

    fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
        let now = tokio::time::Instant::now();
>>>>>>> origin/main
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
                self.event_waker.wake();
            }
            FromSwarm::ConnectionEstablished(ConnectionEstablished { peer_id, .. })
                if peer_id == self.bootstrap_peer_id =>
            {
                self.is_connected_to_bootstrap_peer = true;
                self.is_dialing_to_bootstrap_peer = false;
                // Reset retry dial strategy to original values since we succeeded in dialing
                self.bootstrap_dial_retry_strategy = self.bootstrap_dial_retry_config.strategy();
                self.event_waker.wake();
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
                self.event_waker.wake()
            }
            FromSwarm::AddressChange(AddressChange { peer_id, .. })
                if peer_id == self.bootstrap_peer_id =>
            {
                todo!();
            }
            _ => {}
        }
    }

<<<<<<< HEAD
||||||| 0c5c7df2e
    fn on_connection_handler_event(
        &mut self,
        _peer_id: PeerId,
        _connection_id: ConnectionId,
        _event: <Self::ConnectionHandler as ConnectionHandler>::ToBehaviour,
    ) {
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, <Self::ConnectionHandler as ConnectionHandler>::FromBehaviour>>
    {
        let now = tokio::time::Instant::now();

        if self.is_connected_to_bootstrap_peer && !self.is_bootstrap_in_kad_routing_table {
            self.is_bootstrap_in_kad_routing_table = true;
            return Poll::Ready(ToSwarm::GenerateEvent(
                ToOtherBehaviourEvent::FoundListenAddresses {
                    peer_id: self.bootstrap_peer_id,
                    listen_addresses: vec![self.bootstrap_peer_address.clone()],
                },
            ));
        }

        let should_dial =
            !(self.is_dialing_to_bootstrap_peer) && !(self.is_connected_to_bootstrap_peer);

        if should_dial && (self.time_for_next_bootstrap_dial <= now) {
            self.is_dialing_to_bootstrap_peer = true;
            return Poll::Ready(ToSwarm::Dial {
                opts: DialOpts::peer_id(self.bootstrap_peer_id)
                        .addresses(vec![self.bootstrap_peer_address.clone()])
                        // The peer manager might also be dialing to the bootstrap node.
                        .condition(PeerCondition::DisconnectedAndNotDialing)
                        .build(),
            });
        }

        if should_dial {
            // TODO(Andrew): also wake if should_dial is false and we got connected to or
            // disconnected from the bootstrap peer
            configure_context_to_wake_at_instant(cx, self.time_for_next_bootstrap_dial);
        }
        Poll::Pending
    }
}

impl BootstrappingBehaviour {
=======
    fn on_connection_handler_event(
        &mut self,
        _peer_id: PeerId,
        _connection_id: ConnectionId,
        _event: <Self::ConnectionHandler as ConnectionHandler>::ToBehaviour,
    ) {
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, <Self::ConnectionHandler as ConnectionHandler>::FromBehaviour>>
    {
        let now = tokio::time::Instant::now();
        self.event_waker.add_waker(cx.waker());
        let _ = self.time_waker.poll_unpin(cx);

        if self.is_connected_to_bootstrap_peer && !self.is_bootstrap_in_kad_routing_table {
            self.is_bootstrap_in_kad_routing_table = true;
            return Poll::Ready(ToSwarm::GenerateEvent(
                ToOtherBehaviourEvent::FoundListenAddresses {
                    peer_id: self.bootstrap_peer_id,
                    listen_addresses: vec![self.bootstrap_peer_address.clone()],
                },
            ));
        }

        let should_dial =
            !(self.is_dialing_to_bootstrap_peer) && !(self.is_connected_to_bootstrap_peer);

        if should_dial && (self.time_for_next_bootstrap_dial <= now) {
            self.is_dialing_to_bootstrap_peer = true;
            return Poll::Ready(ToSwarm::Dial {
                opts: DialOpts::peer_id(self.bootstrap_peer_id)
                        .addresses(vec![self.bootstrap_peer_address.clone()])
                        // The peer manager might also be dialing to the bootstrap node.
                        .condition(PeerCondition::DisconnectedAndNotDialing)
                        .build(),
            });
        }

        if should_dial {
            let next_wake_up = self.time_for_next_bootstrap_dial;
            self.time_waker.wake_at(cx, next_wake_up);
        }

        Poll::Pending
    }
}

impl BootstrappingBehaviour {
>>>>>>> origin/main
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
            time_waker: Default::default(),
            event_waker: Default::default(),
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
        self.event_waker.add_waker(cx.waker());
        let _ = self.time_waker.poll_unpin(cx);

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
            self.time_waker.wake_at(cx, next_wake_up);
        }

        Poll::Pending
    }
}
