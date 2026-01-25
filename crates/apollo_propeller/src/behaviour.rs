//! Propeller network behaviour (libp2p adapter).

use std::task::{Context, Poll};

use libp2p::core::Endpoint;
use libp2p::identity::{PeerId, PublicKey};
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

use crate::config::Config;
use crate::handler::{Handler, HandlerOut};
use crate::types::{Channel, Event, MessageRoot, PeerSetError, ShardPublishError};

/// The Propeller network behaviour.
pub struct Behaviour {
    /// Configuration for this behaviour.
    config: Config,
}

impl Behaviour {
    /// Create a new Propeller behaviour.
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub async fn register_channel_peers(
        &mut self,
        _channel: Channel,
        _peers: Vec<(PeerId, u64)>,
    ) -> Result<(), PeerSetError> {
        // TODO(AndrewL): Forward to engine for channel peer registration
        todo!()
    }

    pub async fn register_channel_peers_and_optional_keys(
        &mut self,
        _channel: Channel,
        _peers: Vec<(PeerId, u64, Option<PublicKey>)>,
    ) -> Result<(), PeerSetError> {
        // TODO(AndrewL): Forward to engine for channel peer registration with optional keys
        todo!()
    }

    pub async fn unregister_channel(&mut self, _channel: Channel) -> Result<(), ()> {
        // TODO(AndrewL): Forward to engine for channel unregistration
        todo!()
    }

    pub async fn broadcast(
        &mut self,
        _channel: Channel,
        _message: Vec<u8>,
    ) -> Result<MessageRoot, ShardPublishError> {
        // TODO(AndrewL): Forward to engine for message broadcasting
        todo!()
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
        Ok(Handler::new(self.config.stream_protocol.clone(), self.config.max_wire_message_size))
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &libp2p::core::Multiaddr,
        _role_override: Endpoint,
        _port_use: libp2p::core::transport::PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(Handler::new(self.config.stream_protocol.clone(), self.config.max_wire_message_size))
    }

    fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
        match event {
            FromSwarm::ConnectionEstablished(ConnectionEstablished { .. }) => {}
            FromSwarm::ConnectionClosed(ConnectionClosed { .. }) => {}
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
        Poll::Pending
    }
}
