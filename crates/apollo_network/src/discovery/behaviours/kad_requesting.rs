use std::task::{Context, Poll};

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
use tokio::time::{Duration, Instant};

use super::configure_context_to_wake_at_instant;
use crate::discovery::ToOtherBehaviourEvent;

pub struct KadRequestingBehaviour {
    heartbeat_interval: Duration,
    time_for_next_kad_query: Instant,
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
        // remember instant
        let now = Instant::now();

        if self.time_for_next_kad_query <= now {
            self.time_for_next_kad_query = now + self.heartbeat_interval;
            return Poll::Ready(ToSwarm::GenerateEvent(ToOtherBehaviourEvent::RequestKadQuery(
                libp2p::identity::PeerId::random(),
            )));
        }

        configure_context_to_wake_at_instant(cx.waker().clone(), self.time_for_next_kad_query);
        Poll::Pending
    }
}

impl KadRequestingBehaviour {
    pub fn new(heartbeat_interval: Duration) -> Self {
        Self { heartbeat_interval, time_for_next_kad_query: Instant::now() }
    }
}
