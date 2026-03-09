use std::collections::{HashSet, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

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
use tokio::time::{Duration, Instant, Sleep};

use crate::discovery::ToOtherBehaviourEvent;

pub struct KadRequestingBehaviour {
    heartbeat_interval: Duration,
    time_for_next_kad_query: Instant,
    sleeper: Option<Pin<Box<Sleep>>>,
    /// All peers we want to be connected to (for fast membership checks).
    target_peers: HashSet<PeerId>,
    /// All peers that have at least one established connection.
    connected_peers: HashSet<PeerId>,
    /// Round-robin queue of target peers not yet connected. Peers are rotated to the back
    /// after being queried, and removed when a connection is established.
    peers_pending_connection: VecDeque<PeerId>,
    /// Stored waker to re-poll when new peers are added via `set_target_peers`.
    waker: Option<Waker>,
    /// Peers to dial after a successful DHT lookup matched a requested peer.
    pending_dials: VecDeque<PeerId>,
}

impl NetworkBehaviour for KadRequestingBehaviour {
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

    fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
        match event {
            FromSwarm::ConnectionEstablished(ConnectionEstablished { peer_id, .. }) => {
                self.connected_peers.insert(peer_id);
                self.peers_pending_connection.retain(|p| *p != peer_id);
            }
            FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                remaining_established: 0,
                ..
            }) => {
                self.connected_peers.remove(&peer_id);
                if self.target_peers.contains(&peer_id) {
                    self.peers_pending_connection.push_back(peer_id);
                }
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
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, <Self::ConnectionHandler as ConnectionHandler>::FromBehaviour>>
    {
        // Drain pending dials first (from DHT lookup results).
        if let Some(peer_id) = self.pending_dials.pop_front() {
            return Poll::Ready(ToSwarm::Dial { opts: peer_id.into() });
        }

        if self.peers_pending_connection.is_empty() {
            self.waker = Some(cx.waker().clone());
            return Poll::Pending;
        }

        let now = Instant::now();
        if now >= self.time_for_next_kad_query {
            return self.emit_heartbeat_query(now);
        }
        if self.sleeper.is_none() {
            self.sleeper = Some(Box::pin(tokio::time::sleep_until(self.time_for_next_kad_query)));
        }
        let sleeper =
            self.sleeper.as_mut().expect("Sleeper cannot be None after being created above.");

        match sleeper.as_mut().poll(cx) {
            Poll::Ready(()) => self.emit_heartbeat_query(now),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl KadRequestingBehaviour {
    pub fn new(heartbeat_interval: Duration) -> Self {
        Self {
            heartbeat_interval,
            time_for_next_kad_query: Instant::now(),
            sleeper: None,
            target_peers: HashSet::new(),
            connected_peers: HashSet::new(),
            peers_pending_connection: VecDeque::new(),
            waker: None,
            pending_dials: VecDeque::new(),
        }
    }

    pub fn set_target_peers(&mut self, peers: HashSet<PeerId>) {
        self.peers_pending_connection =
            peers.iter().filter(|p| !self.connected_peers.contains(p)).copied().collect();
        self.target_peers = peers;
        self.pending_dials.clear();
        if !self.peers_pending_connection.is_empty() {
            if let Some(waker) = self.waker.take() {
                waker.wake();
            }
        }
    }

    pub fn enqueue_dials_for_matching_peers(&mut self, peers: &[PeerId]) {
        let new_dials: Vec<PeerId> = peers
            .iter()
            .copied()
            .filter(|p| self.target_peers.contains(p))
            .filter(|p| !self.connected_peers.contains(p))
            .filter(|p| !self.peers_pending_connection.contains(p))
            .filter(|p| !self.pending_dials.contains(p))
            .collect();
        self.pending_dials.extend(new_dials);
    }

    fn emit_heartbeat_query(
        &mut self,
        now: Instant,
    ) -> Poll<
        ToSwarm<
            ToOtherBehaviourEvent,
            <dummy::ConnectionHandler as ConnectionHandler>::FromBehaviour,
        >,
    > {
        self.time_for_next_kad_query = now + self.heartbeat_interval;
        self.sleeper = Some(Box::pin(tokio::time::sleep_until(self.time_for_next_kad_query)));
        let Some(peer_id) = self.peers_pending_connection.pop_front() else {
            return Poll::Pending;
        };
        self.peers_pending_connection.push_back(peer_id);
        Poll::Ready(ToSwarm::GenerateEvent(ToOtherBehaviourEvent::RequestKadQuery(peer_id)))
    }
}
