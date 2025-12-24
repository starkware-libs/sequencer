//! Propeller network behaviour (libp2p adapter).

use std::task::{Context, Poll};

use libp2p::core::Endpoint;
use libp2p::identity::PeerId;
use libp2p::swarm::behaviour::{ConnectionClosed, ConnectionEstablished, FromSwarm};
use libp2p::swarm::{
    ConnectionDenied,
    ConnectionId,
    NetworkBehaviour,
    THandler,
    THandlerInEvent,
    THandlerOutEvent,
    ToSwarm,
};

use crate::handler::{Handler, HandlerOut};
use crate::types::Event;

/// Maximum message size in bytes (1 MB).
const MAX_MESSAGE_SIZE: usize = 1 << 20;

/// The Propeller network behaviour.
#[derive(Default)]
pub struct Behaviour {}

impl Behaviour {
    /// Create a new Propeller behaviour.
    pub fn new() -> Self {
        Self {}
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = Handler;
    type ToSwarm = Event;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _local_addr: &libp2p::core::Multiaddr,
        _remote_addr: &libp2p::core::Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(Handler::new(libp2p::swarm::StreamProtocol::new("/propeller/0.1.0"), MAX_MESSAGE_SIZE))
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &libp2p::core::Multiaddr,
        _role_override: Endpoint,
        _port_use: libp2p::core::transport::PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(Handler::new(libp2p::swarm::StreamProtocol::new("/propeller/0.1.0"), MAX_MESSAGE_SIZE))
    }

    fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
        match event {
            FromSwarm::ConnectionEstablished(ConnectionEstablished { .. }) => {
                // TODO(AndrewL): Handle connection establishment
            }
            FromSwarm::ConnectionClosed(ConnectionClosed { .. }) => {
                // TODO(AndrewL): Handle connection closure
            }
            _ => {}
        }
    }

    fn on_connection_handler_event(
        &mut self,
        _peer_id: PeerId,
        _connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        match event {
            HandlerOut::Unit(_unit) => {
                // TODO(AndrewL): Forward to engine for validation
            }
            HandlerOut::SendError(_error) => {
                // TODO(AndrewL): Forward to engine for error handling
            }
        }
    }

    fn poll(
        &mut self,
        _cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        // TODO(AndrewL): Return the first (if exists) of any pending events
        Poll::Pending
    }
}
