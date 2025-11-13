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
use tracing::info;

use crate::network_manager::metrics::{EventMetrics, EventType};

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
                self.metrics.increment_event(EventType::ConnectionsEstablished);
            }
            FromSwarm::ConnectionClosed(_) => {
                self.metrics.increment_event(EventType::ConnectionsClosed);
            }
            FromSwarm::DialFailure(_) => {
                self.metrics.increment_event(EventType::DialFailure);
            }
            FromSwarm::ListenFailure(_) => {
                self.metrics.increment_event(EventType::ListenFailure);
            }
            FromSwarm::ListenerError(_) => {
                self.metrics.increment_event(EventType::ListenError);
            }
            FromSwarm::AddressChange(_) => {
                self.metrics.increment_event(EventType::AddressChange);
            }
            FromSwarm::NewListener(_) => {
                self.metrics.increment_event(EventType::NewListeners);
            }
            FromSwarm::NewListenAddr(_) => {
                self.metrics.increment_event(EventType::NewListenAddrs);
            }
            FromSwarm::ExpiredListenAddr(_) => {
                self.metrics.increment_event(EventType::ExpiredListenAddrs);
            }
            FromSwarm::ListenerClosed(_) => {
                self.metrics.increment_event(EventType::ListenerClosed);
            }
            FromSwarm::NewExternalAddrCandidate(_) => {
                self.metrics.increment_event(EventType::NewExternalAddrCandidate);
            }
            FromSwarm::ExternalAddrConfirmed(_) => {
                self.metrics.increment_event(EventType::ExternalAddrConfirmed);
            }
            FromSwarm::ExternalAddrExpired(_) => {
                self.metrics.increment_event(EventType::ExternalAddrExpired);
            }
            FromSwarm::NewExternalAddrOfPeer(_) => {
                self.metrics.increment_event(EventType::NewExternalAddrOfPeer);
            }
            _ => {
                // ignore other events
                return;
            }
        }
        // log the event
        info!(?event, "Swarm event");
    }
}

impl NetworkBehaviour for EventMetricsTracker {
    type ConnectionHandler = libp2p::swarm::dummy::ConnectionHandler;
    type ToSwarm = std::convert::Infallible;

    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        info!(?connection_id, ?peer, ?local_addr, ?remote_addr, "Inbound connection established");
        self.metrics.increment_event(EventType::InboundConnectionsHandled);
        Ok(libp2p::swarm::dummy::ConnectionHandler)
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        addr: &Multiaddr,
        role_override: libp2p::core::Endpoint,
        port_use: libp2p::core::transport::PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        info!(
            ?connection_id,
            ?peer,
            ?addr,
            ?role_override,
            ?port_use,
            "Outbound connection established"
        );
        self.metrics.increment_event(EventType::OutboundConnectionsHandled);
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
        self.metrics.increment_event(EventType::ConnectionHandlerEvents);
    }

    fn poll(
        &mut self,
        _cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        Poll::Pending
    }
}
