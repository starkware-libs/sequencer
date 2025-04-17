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

use crate::discovery::ToOtherBehaviourEvent;

const DEBUG: bool = false;

pub struct KadRequestingBehaviour {
    heartbeat_interval: Duration,
    number_of_connections: usize,
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

    fn on_swarm_event(&mut self, e: FromSwarm<'_>) {
        if DEBUG {
            println!("EVENT {:?} - KadRequestingBehaviour - {:?}", Instant::now(), e);
        }

        match e {
            FromSwarm::ConnectionEstablished(_) => {
                self.number_of_connections += 1;
            }
            FromSwarm::ConnectionClosed(_) => {
                self.number_of_connections = self.number_of_connections.checked_sub(1).unwrap();
            }
            _ => todo!(),
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
        _: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, <Self::ConnectionHandler as ConnectionHandler>::FromBehaviour>>
    {
        // remember instant
        let now = Instant::now();

        let r = if (self.time_for_next_kad_query <= now) && (self.number_of_connections > 0) {
            self.time_for_next_kad_query = now + self.heartbeat_interval;
            Poll::Ready(ToSwarm::GenerateEvent(ToOtherBehaviourEvent::RequestKadQuery(
                libp2p::identity::PeerId::random(),
            )))
        } else {
            Poll::Pending
        };

        if DEBUG {
            println!("POLL RESULT {:?} - KadRequestingBehaviour - {:?}", Instant::now(), r);
        }

        r
    }
}

impl KadRequestingBehaviour {
    pub fn new(heartbeat_interval: Duration) -> Self {
        Self {
            heartbeat_interval,
            time_for_next_kad_query: Instant::now(),
            number_of_connections: 0,
        }
    }
}
