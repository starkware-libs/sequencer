use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
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
use tokio::time::{Duration, Instant, Sleep};

use crate::discovery::ToOtherBehaviourEvent;

pub struct KadRequestingBehaviour {
    heartbeat_interval: Duration,
    time_for_next_kad_query: Instant,
    sleeper: Option<Pin<Box<Sleep>>>,
    /// Peers to include in Kademlia queries. Drained one at a time during polling.
    peers_to_request: HashSet<PeerId>,
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

    fn on_swarm_event(&mut self, _: FromSwarm<'_>) {}

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
        // Drain pending peer requests first, one per poll call.
        if let Some(peer_id) = self.peers_to_request.iter().next().copied() {
            self.peers_to_request.remove(&peer_id);
            return Poll::Ready(ToSwarm::GenerateEvent(ToOtherBehaviourEvent::RequestKadQuery(
                peer_id,
            )));
        }

        let now = Instant::now();
        if now >= self.time_for_next_kad_query {
            // No need to deal with sleep.
            return self.set_for_next_kad_query(now);
        }
        if self.sleeper.is_none() {
            self.sleeper = Some(Box::pin(tokio::time::sleep_until(self.time_for_next_kad_query)));
        }
        let sleeper =
            self.sleeper.as_mut().expect("Sleeper cannot be None after being created above.");

        match sleeper.as_mut().poll(cx) {
            Poll::Ready(()) => self.set_for_next_kad_query(now),
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
            peers_to_request: HashSet::new(),
        }
    }

    pub fn set_peers_to_request(&mut self, peers: HashSet<PeerId>) {
        self.peers_to_request = peers;
    }

    fn set_for_next_kad_query(
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
        Poll::Ready(ToSwarm::GenerateEvent(ToOtherBehaviourEvent::RequestKadQuery(
            libp2p::identity::PeerId::random(),
        )))
    }
}
