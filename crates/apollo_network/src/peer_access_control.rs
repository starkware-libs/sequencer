use std::collections::{HashSet, VecDeque};
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
pub struct PeerNotAllowed(PeerId);

pub struct Behaviour {
    bootstrap_peer_ids: HashSet<PeerId>,
    target_peers: HashSet<PeerId>,
    connected_peer_ids: HashSet<PeerId>,
    enforcement_active: bool,
    pending_disconnections: VecDeque<PeerId>,
    waker: Option<Waker>,
}

impl Behaviour {
    pub fn new(bootstrap_peer_ids: HashSet<PeerId>) -> Self {
        Self {
            bootstrap_peer_ids,
            target_peers: HashSet::new(),
            connected_peer_ids: HashSet::new(),
            enforcement_active: false,
            pending_disconnections: VecDeque::new(),
            waker: None,
        }
    }

    pub fn set_target_peers(&mut self, peers: HashSet<PeerId>) {
        self.enforcement_active = true;
        self.target_peers = peers;

        let peers_to_disconnect: Vec<PeerId> = self
            .connected_peer_ids
            .iter()
            .filter(|peer_id| !self.is_peer_allowed(peer_id))
            .copied()
            .collect();

        for peer_id in peers_to_disconnect {
            if !self.pending_disconnections.contains(&peer_id) {
                info!(%peer_id, "Queuing disconnection for peer no longer in allowed set");
                self.pending_disconnections.push_back(peer_id);
            }
        }

        if !self.pending_disconnections.is_empty() {
            if let Some(waker) = self.waker.take() {
                waker.wake();
            }
        }
    }

    fn is_peer_allowed(&self, peer_id: &PeerId) -> bool {
        !self.enforcement_active
            || self.target_peers.contains(peer_id)
            || self.bootstrap_peer_ids.contains(peer_id)
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = dummy::ConnectionHandler;
    type ToSwarm = Infallible;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        if self.is_peer_allowed(&peer) {
            Ok(dummy::ConnectionHandler)
        } else {
            Err(ConnectionDenied::new(PeerNotAllowed(peer)))
        }
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

    fn on_connection_handler_event(
        &mut self,
        _peer_id: PeerId,
        _connection_id: ConnectionId,
        _event: <Self::ConnectionHandler as libp2p::swarm::ConnectionHandler>::ToBehaviour,
    ) {
    }

    fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
        match event {
            FromSwarm::ConnectionEstablished(info) => {
                self.connected_peer_ids.insert(info.peer_id);
            }
            FromSwarm::ConnectionClosed(info) if info.remaining_established == 0 => {
                self.connected_peer_ids.remove(&info.peer_id);
            }
            _ => {}
        }
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<
        ToSwarm<
            Self::ToSwarm,
            <Self::ConnectionHandler as libp2p::swarm::ConnectionHandler>::FromBehaviour,
        >,
    > {
        if let Some(peer_id) = self.pending_disconnections.pop_front() {
            return Poll::Ready(ToSwarm::CloseConnection {
                peer_id,
                connection: CloseConnection::All,
            });
        }

        self.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}
