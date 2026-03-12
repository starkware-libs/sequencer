use std::collections::HashMap;
use std::task::{Context, Poll};

use libp2p::core::transport::PortUse;
use libp2p::core::Endpoint;
use libp2p::swarm::{
    dummy,
    ConnectionDenied,
    ConnectionHandler,
    ConnectionId,
    FromSwarm,
    NetworkBehaviour,
    ToSwarm,
};
use libp2p::{Multiaddr, PeerId};

use crate::discovery::{RetryConfig, ToOtherBehaviourEvent};

/// Manages dialling to a dynamic set of peers using explicit multiaddresses,
/// with exponential backoff on failure.
///
/// Does not re-dial peers after disconnection — callers must re-request if
/// reconnection is desired.
// TODO(AndrewL): remove this once the behaviour is added
#[allow(dead_code)]
pub struct DiallingBehaviour {
    retry_config: RetryConfig,
    peers: HashMap<PeerId, Vec<Multiaddr>>,
}

// TODO(AndrewL): remove this once the behaviour is added
#[allow(dead_code)]
impl DiallingBehaviour {
    pub fn new(retry_config: RetryConfig) -> Self {
        Self { retry_config, peers: HashMap::new() }
    }

    /// Request dialling a peer at the given addresses.
    ///
    /// If the peer is already connected or being dialled, the addresses are updated but no
    /// new dial is initiated.
    pub fn request_dial(&mut self, _peer_id: PeerId, _addresses: Vec<Multiaddr>) {
        todo!()
    }

    /// Cancel any pending or in-progress dial for a peer and stop tracking it.
    pub fn cancel_dial(&mut self, _peer_id: &PeerId) {
        todo!()
    }
}

impl NetworkBehaviour for DiallingBehaviour {
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
        _port_use: PortUse,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        Ok(dummy::ConnectionHandler)
    }

    fn on_swarm_event(&mut self, _event: FromSwarm<'_>) {
        todo!()
    }

    fn on_connection_handler_event(
        &mut self,
        _peer_id: PeerId,
        _connection_id: ConnectionId,
        _event: <Self::ConnectionHandler as ConnectionHandler>::ToBehaviour,
    ) {
    }

    fn poll(
        &mut self,
        _cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, <Self::ConnectionHandler as ConnectionHandler>::FromBehaviour>>
    {
        todo!()
    }
}
