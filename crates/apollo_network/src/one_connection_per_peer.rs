use std::collections::HashSet;
use std::task::{Context, Poll};

use libp2p::core::transport::PortUse;
use libp2p::core::Endpoint;
use libp2p::swarm::behaviour::ConnectionEstablished;
use libp2p::swarm::{
    dummy,
    ConnectionClosed,
    ConnectionDenied,
    ConnectionHandler,
    ConnectionId,
    FromSwarm,
    NetworkBehaviour,
    ToSwarm,
};
use libp2p::{Multiaddr, PeerId};

use crate::discovery::ToOtherBehaviourEvent;

#[derive(Debug, Clone, Default)]
pub struct OneConnectionPerPeerBehaviour {
    connected_peers: HashSet<PeerId>,
}

impl NetworkBehaviour for OneConnectionPerPeerBehaviour {
    type ConnectionHandler = dummy::ConnectionHandler;
    type ToSwarm = ToOtherBehaviourEvent;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        if self.connected_peers.contains(&peer) {
            return Err(ConnectionDenied::new("Peer already has an established connection"));
        }
        Ok(dummy::ConnectionHandler)
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
        _port_use: PortUse,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        if self.connected_peers.contains(&peer) {
            return Err(ConnectionDenied::new("Peer already has an established connection"));
        }
        Ok(dummy::ConnectionHandler)
    }

    fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
        match event {
            FromSwarm::ConnectionEstablished(ConnectionEstablished {
                peer_id,
                other_established,
                connection_id,
                endpoint,
                failed_addresses,
            }) => {
                tracing::info!(
                    "Connection established with peer: {peer_id}, other_established: \
                     {other_established}, connection_id: {connection_id:?}, endpoint: \
                     {endpoint:?}, failed_addresses: {failed_addresses:?}",
                );
                assert_eq!(
                    other_established, 0,
                    "Multiple connections established with the same peer are not allowed, how did \
                     this happen?"
                );
                self.connected_peers.insert(peer_id);
            }
            FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                remaining_established,
                connection_id,
                endpoint,
                cause,
            }) => {
                tracing::info!(
                    "Connection closed with peer: {peer_id}, remaining connections: \
                     {remaining_established}, connection_id: {connection_id:?}, endpoint: \
                     {endpoint:?}, cause: {cause:?}",
                );
                assert_eq!(
                    remaining_established, 0,
                    "Multiple connections closed from the same peer are not allowed, how did this \
                     happen?"
                );
                self.connected_peers.remove(&peer_id);
            }
            _ => {}
        }
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
        // This behavior doesn't generate any events, it only prevents multiple connections
        Poll::Pending
    }
}
