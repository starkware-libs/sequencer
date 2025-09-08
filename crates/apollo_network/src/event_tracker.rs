use std::task::{Context, Poll};

use libp2p::swarm::{
    ConnectionDenied,
    ConnectionId,
    FromSwarm,
    NetworkBehaviour,
    THandler,
    THandlerInEvent,
    THandlerOutEvent,
    ToSwarm,
};
use libp2p::{Multiaddr, PeerId};

use crate::network_manager::metrics::EventMetrics;

/// A behavior that tracks all network events and updates metrics accordingly.
/// This behavior does not generate any events of its own and has no connection handler.
/// It only observes and updates metrics based on events from other behaviors.
pub struct EventMetricsTracker {
    metrics: EventMetrics,
}

impl EventMetricsTracker {
    pub fn new(metrics: EventMetrics) -> Self {
        Self { metrics }
    }

    fn track_swarm_event(&mut self, event: &FromSwarm<'_>) {
        match event {
            FromSwarm::ConnectionEstablished(_) => {
                self.metrics.connections_established.increment(1);
            }
            FromSwarm::ConnectionClosed(_) => {
                self.metrics.connections_closed.increment(1);
            }
            FromSwarm::DialFailure(_) => {
                self.metrics.dial_failure.increment(1);
            }
            FromSwarm::ListenFailure(_) => {
                self.metrics.listen_failure.increment(1);
            }
            FromSwarm::ListenerError(_) => {
                self.metrics.listen_error.increment(1);
            }
            FromSwarm::AddressChange(_) => {
                self.metrics.address_change.increment(1);
            }
            FromSwarm::NewListener(_) => {
                self.metrics.new_listeners.increment(1);
            }
            FromSwarm::NewListenAddr(_) => {
                self.metrics.new_listen_addrs.increment(1);
            }
            FromSwarm::ExpiredListenAddr(_) => {
                self.metrics.expired_listen_addrs.increment(1);
            }
            FromSwarm::ListenerClosed(_) => {
                self.metrics.listener_closed.increment(1);
            }
            FromSwarm::NewExternalAddrCandidate(_) => {
                self.metrics.new_external_addr_candidate.increment(1);
            }
            FromSwarm::ExternalAddrConfirmed(_) => {
                self.metrics.external_addr_confirmed.increment(1);
            }
            FromSwarm::ExternalAddrExpired(_) => {
                self.metrics.external_addr_expired.increment(1);
            }
            FromSwarm::NewExternalAddrOfPeer(_) => {
                self.metrics.new_external_addr_of_peer.increment(1);
            }
            _ => {}
        }
    }
}

impl NetworkBehaviour for EventMetricsTracker {
    type ConnectionHandler = libp2p::swarm::dummy::ConnectionHandler;
    type ToSwarm = std::convert::Infallible;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.metrics.inbound_connections_handled.increment(1);
        Ok(libp2p::swarm::dummy::ConnectionHandler)
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &Multiaddr,
        _role_override: libp2p::core::Endpoint,
        _port_use: libp2p::core::transport::PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.metrics.outbound_connections_handled.increment(1);
        Ok(libp2p::swarm::dummy::ConnectionHandler)
    }

    fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
        self.track_swarm_event(&event);
    }

    fn on_connection_handler_event(
        &mut self,
        _peer_id: PeerId,
        _connection_id: ConnectionId,
        _event: THandlerOutEvent<Self>,
    ) {
        self.metrics.connection_handler_events.increment(1);
    }

    fn poll(
        &mut self,
        _cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        Poll::Pending
    }
}
