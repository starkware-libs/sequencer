use std::collections::{HashMap, HashSet};
use std::task::{Context, Poll};

use libp2p::core::Endpoint;
use libp2p::swarm::behaviour::ConnectionEstablished;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::swarm::{
    dummy,
    ConnectionClosed,
    ConnectionDenied,
    ConnectionHandler,
    ConnectionId,
    DialFailure,
    FromSwarm,
    ListenFailure,
    NetworkBehaviour,
    ToSwarm,
};
use libp2p::{Multiaddr, PeerId};
use tracing::{error, info};

use crate::discovery::ToOtherBehaviourEvent;

#[derive(Debug, Clone, Default)]
pub struct OneConnectionPerPeerBehaviour {
    connected_peers: HashSet<PeerId>,
    pending_connections: HashMap<PeerId, usize>,
}

impl OneConnectionPerPeerBehaviour {
    /// Returns a reference to the set of connected peers (for testing)
    #[cfg(test)]
    pub fn connected_peers(&self) -> &HashSet<PeerId> {
        &self.connected_peers
    }

    fn add_pending_connection(&mut self, peer: PeerId) {
        *self.pending_connections.entry(peer).or_insert(0) += 1;
    }

    fn get_pending_connection_count(&self, peer: PeerId) -> usize {
        *self.pending_connections.get(&peer).unwrap_or(&0)
    }

    fn remove_pending_connection(&mut self, peer: PeerId) {
        if let Some(count) = self.pending_connections.get_mut(&peer) {
            *count -= 1;
            if *count == 0 {
                self.pending_connections.remove(&peer);
            }
        }
    }
}

impl OneConnectionPerPeerBehaviour {
    fn handle_established_connection(
        &mut self,
        peer: PeerId,
    ) -> Result<dummy::ConnectionHandler, ConnectionDenied> {
        self.add_pending_connection(peer);
        if self.connected_peers.contains(&peer) {
            info!(
                "OneConnectionPerPeerBehaviour::handle_established_connection - connection denied"
            );
            return Err(ConnectionDenied::new("Peer already has an established connection"));
        }
        if self.get_pending_connection_count(peer) > 1 {
            info!(
                "OneConnectionPerPeerBehaviour::handle_established_connection - connection \
                 denied, peer has multiple pending connections"
            );
            return Err(ConnectionDenied::new("Peer already has multiple pending connections"));
        }
        Ok(dummy::ConnectionHandler)
    }
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
        self.handle_established_connection(peer)
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        self.handle_established_connection(peer)
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
                let trace_message = format!(
                    "Connection established with peer: {peer_id}, other_established: \
                     {other_established}, connection_id: {connection_id:?}, endpoint: \
                     {endpoint:?}, failed_addresses: {failed_addresses:?}"
                );
                info!(trace_message);
                if other_established != 0 {
                    error!(
                        "Multiple connections established with the same peer are not allowed. \
                         {trace_message}" /* adding this in case not running with info */
                    );
                }
                self.connected_peers.insert(peer_id);
                self.remove_pending_connection(peer_id);
            }
            FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                remaining_established,
                connection_id,
                endpoint,
            }) => {
                let trace_message = format!(
                    "Connection closed with peer: {peer_id}, remaining connections: \
                     {remaining_established}, connection_id: {connection_id:?}, endpoint: \
                     {endpoint:?}"
                );
                info!(trace_message);
                if remaining_established != 0 {
                    error!(
                        "Connection closed with remaining established connections. {trace_message}"
                    );
                }
                let was_present = self.connected_peers.remove(&peer_id);
                if !was_present {
                    error!("Connection closed for a peer that was not connected. {trace_message}");
                }
            }
            FromSwarm::DialFailure(DialFailure { peer_id, error, connection_id }) => {
                let trace_message = format!(
                    "Dial failure with peer: {peer_id:?}, error: {error:?}, connection_id: \
                     {connection_id:?}"
                );
                info!(trace_message);
                if let Some(peer) = peer_id {
                    self.remove_pending_connection(peer);
                } else {
                    info!("Dial failure without a peer_id, connection_id: {connection_id:?}");
                }
            }
            FromSwarm::ListenFailure(ListenFailure {
                local_addr,
                send_back_addr,
                error,
                connection_id,
            }) => {
                let trace_message = format!(
                    "Listen failure with local_addr: {local_addr:?}, send_back_addr: \
                     {send_back_addr:?}, error: {error:?}, connection_id: {connection_id:?}"
                );
                info!(trace_message);
                // Extract peer_id from multiaddr if it contains one
                if let Some(peer_id) = DialOpts::from(send_back_addr.clone()).get_peer_id() {
                    self.remove_pending_connection(peer_id);
                } else {
                    info!("Listen failure without a peer_id, connection_id: {connection_id:?}");
                }
            }
            event => {
                info!("OneConnectionPerPeerBehaviour::on_swarm_event - unhandled event: {event:?}");
            }
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
