use std::collections::HashSet;
use std::convert::Infallible;
use std::task::{Context, Poll, Waker};

use libp2p::core::transport::PortUse;
use libp2p::core::Endpoint;
use libp2p::swarm::{
    dummy,
    CloseConnection,
    ConnectionDenied,
    ConnectionId,
    FromSwarm,
    NetworkBehaviour,
    ToSwarm,
};
use libp2p::{Multiaddr, PeerId};
use tracing::info;

#[derive(Debug, thiserror::Error)]
#[error("Peer {0} is not in the allowed peers set.")]
pub struct PeerNotAllowedError(PeerId);

/// Enforces a peer whitelist by denying connections from peers not in the allowed set and
/// requesting disconnection of peers that are removed from the allowed set.
///
/// Inbound connections are denied at the established stage (the earliest point where the peer's
/// identity is known after the noise handshake). Outbound connections are denied at the pending
/// stage when the peer ID is available.
///
/// Until [`set_allowed_peers`](Behaviour::set_allowed_peers) is called the allowed set is empty,
/// so all connections are denied.
pub struct Behaviour {
    allowed_peers: HashSet<PeerId>,
    pending_disconnections: Vec<PeerId>,
    waker: Option<Waker>,
}

impl Behaviour {
    pub fn new() -> Self {
        Self { allowed_peers: HashSet::new(), pending_disconnections: Vec::new(), waker: None }
    }

    pub fn set_allowed_peers(&mut self, peers: HashSet<PeerId>) {
        // Peers in the old set but not the new set should be disconnected. Requesting
        // disconnection for a peer that is not currently connected is a no-op in libp2p.
        let newly_disallowed: Vec<PeerId> =
            self.allowed_peers.difference(&peers).copied().collect();

        for peer_id in newly_disallowed {
            if !self.pending_disconnections.contains(&peer_id) {
                info!(%peer_id, "Queuing disconnection for peer no longer in allowed set");
                self.pending_disconnections.push(peer_id);
            }
        }

        self.allowed_peers = peers;

        if !self.pending_disconnections.is_empty() {
            if let Some(waker) = self.waker.take() {
                waker.wake();
            }
        }
    }

    fn is_peer_allowed(&self, peer_id: &PeerId) -> bool {
        self.allowed_peers.contains(peer_id)
    }

    fn deny_if_not_allowed(
        &self,
        peer: PeerId,
    ) -> Result<dummy::ConnectionHandler, ConnectionDenied> {
        if self.is_peer_allowed(&peer) {
            Ok(dummy::ConnectionHandler)
        } else {
            Err(ConnectionDenied::new(PeerNotAllowedError(peer)))
        }
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = dummy::ConnectionHandler;
    type ToSwarm = Infallible;

    // Inbound connections can only be checked at the established stage because the peer's identity
    // is not yet known during `handle_pending_inbound_connection` (it is revealed after the noise
    // handshake completes).
    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        self.deny_if_not_allowed(peer)
    }

    fn handle_pending_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        maybe_peer: Option<PeerId>,
        _addresses: &[Multiaddr],
        _effective_role: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        if let Some(peer) = maybe_peer {
            self.deny_if_not_allowed(peer)?;
        }
        Ok(vec![])
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
        _port_use: PortUse,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        self.deny_if_not_allowed(peer)
    }

    fn on_connection_handler_event(
        &mut self,
        _peer_id: PeerId,
        _connection_id: ConnectionId,
        _event: <Self::ConnectionHandler as libp2p::swarm::ConnectionHandler>::ToBehaviour,
    ) {
    }

    fn on_swarm_event(&mut self, _event: FromSwarm<'_>) {}

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<
        ToSwarm<
            Self::ToSwarm,
            <Self::ConnectionHandler as libp2p::swarm::ConnectionHandler>::FromBehaviour,
        >,
    > {
        // CloseConnection is always accepted by the swarm — it never fails or returns an error.
        // If the peer is not currently connected the event is silently ignored.
        if let Some(peer_id) = self.pending_disconnections.pop() {
            return Poll::Ready(ToSwarm::CloseConnection {
                peer_id,
                connection: CloseConnection::All,
            });
        }

        self.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}
