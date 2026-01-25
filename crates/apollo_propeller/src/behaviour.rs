//! Propeller network behaviour (libp2p adapter).
//!
//! This module implements the libp2p `NetworkBehaviour` trait for the Propeller protocol,
//!
//! # Overview
//!
//! The Propeller protocol uses erasure coding and tree-based routing to broadcast messages
//! efficiently across a network of peers. The `Behaviour` struct serves as the main interface
//! between libp2p's networking stack and the Propeller protocol engine.

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
///
/// This struct implements the libp2p `NetworkBehaviour` trait and serves as the main entry
/// point for interacting with the Propeller protocol. It manages channel registrations,
/// message broadcasting, and coordination with the underlying protocol engine.
pub struct Behaviour {
    /// Configuration for this behaviour.
    config: Config,
}

impl Behaviour {
    /// Create a new Propeller behaviour.
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    // TODO(AndrewL): change register channel to return the channel id.

    /// Register peers for a channel where public keys are embedded in peer IDs (Ed25519,
    /// Secp256k1).
    ///
    /// Use when all peers have public keys embedded in their peer IDs. For peers with RSA keys,
    /// use `register_channel_peers_and_optional_keys` instead.
    pub async fn register_channel_peers(
        &mut self,
        _channel: Channel,
        _peers: Vec<(PeerId, u64)>,
    ) -> Result<(), PeerSetError> {
        // TODO(AndrewL): Forward to engine for channel peer registration
        todo!()
    }

    /// Register peers for a channel with optional explicit public keys.
    ///
    /// Use when some peers require explicit public keys (e.g., RSA peer IDs where keys cannot be
    /// derived from peer IDs). Provide `None` for peers with embedded keys (Ed25519, Secp256k1).
    pub async fn register_channel_peers_and_optional_keys(
        &mut self,
        _channel: Channel,
        _peers: Vec<(PeerId, u64, Option<PublicKey>)>,
    ) -> Result<(), PeerSetError> {
        // TODO(AndrewL): Forward to engine for channel peer registration with optional keys
        todo!()
    }

    /// Unregister a channel and clean up all associated state.
    pub async fn unregister_channel(&mut self, _channel: Channel) -> Result<(), ()> {
        // TODO(AndrewL): Forward to engine for channel unregistration
        todo!()
    }

    /// Broadcast a message to all peers in a channel using erasure coding.
    ///
    /// Returns the Merkle root hash of the message, which serves as a unique identifier.
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
