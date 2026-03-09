use std::collections::{HashSet, VecDeque};
use std::future::Future;
use std::pin::Pin;
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
use tokio::time::{Duration, Instant, Sleep};

use crate::discovery::ToOtherBehaviourEvent;

pub struct KadRequestingBehaviour {
    heartbeat_interval: Duration,
    time_for_next_kad_query: Instant,
    sleeper: Option<Pin<Box<Sleep>>>,
    /// When true, periodic Kademlia queries for random peers are emitted on each heartbeat.
    /// When false, only explicitly requested peers (via `set_peers_to_request`) are queried.
    random_peer_request_enabled: bool,
    /// Peers we want to be connected to (for fast membership checks).
    peers_to_request: HashSet<PeerId>,
    /// Requested peers that have at least one established connection.
    connected_peers: HashSet<PeerId>,
    /// Round-robin queue of requested peers not yet connected. Peers are rotated to the back
    /// after being queried, and removed when a connection is established.
    peers_pending_connection: VecDeque<PeerId>,
    /// Buffer of queries to emit for the current heartbeat, drained one per poll call.
    pending_queries: VecDeque<PeerId>,
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
                if self.peers_to_request.contains(&peer_id) {
                    self.connected_peers.insert(peer_id);
                }
                self.peers_pending_connection.retain(|p| *p != peer_id);
            }
            FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                remaining_established: 0,
                ..
            }) => {
                self.connected_peers.remove(&peer_id);
                if self.peers_to_request.contains(&peer_id) {
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
        // Drain pending queries from the current heartbeat, one per poll call.
        if let Some(peer_id) = self.pending_queries.pop_front() {
            return Poll::Ready(ToSwarm::GenerateEvent(ToOtherBehaviourEvent::RequestKadQuery(
                peer_id,
            )));
        }

        if !self.random_peer_request_enabled && self.peers_pending_connection.is_empty() {
            return Poll::Pending;
        }

        let now = Instant::now();
        if now >= self.time_for_next_kad_query {
            return self.emit_heartbeat_queries(now);
        }
        if self.sleeper.is_none() {
            self.sleeper = Some(Box::pin(tokio::time::sleep_until(self.time_for_next_kad_query)));
        }
        let sleeper =
            self.sleeper.as_mut().expect("Sleeper cannot be None after being created above.");

        match sleeper.as_mut().poll(cx) {
            Poll::Ready(()) => self.emit_heartbeat_queries(now),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl KadRequestingBehaviour {
    pub fn new(heartbeat_interval: Duration, random_peer_request_enabled: bool) -> Self {
        Self {
            heartbeat_interval,
            time_for_next_kad_query: Instant::now(),
            sleeper: None,
            random_peer_request_enabled,
            peers_to_request: HashSet::new(),
            connected_peers: HashSet::new(),
            peers_pending_connection: VecDeque::new(),
            pending_queries: VecDeque::new(),
        }
    }

    pub fn set_peers_to_request(&mut self, peers: HashSet<PeerId>) {
        self.connected_peers.retain(|p| peers.contains(p));
        self.peers_pending_connection =
            peers.iter().filter(|p| !self.connected_peers.contains(p)).copied().collect();
        self.peers_to_request = peers;
    }

    fn emit_heartbeat_queries(
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
        // Query one peer pending connection, rotating through the queue.
        if let Some(peer_id) = self.peers_pending_connection.pop_front() {
            self.peers_pending_connection.push_back(peer_id);
            self.pending_queries.push_back(peer_id);
        }
        if self.random_peer_request_enabled {
            self.pending_queries.push_back(PeerId::random());
        }
        match self.pending_queries.pop_front() {
            Some(peer_id) => Poll::Ready(ToSwarm::GenerateEvent(
                ToOtherBehaviourEvent::RequestKadQuery(peer_id),
            )),
            None => Poll::Pending,
        }
    }
}
