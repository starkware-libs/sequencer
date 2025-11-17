//! This module is responsible for monitoring connection health using ping and disconnecting
//! unhealthy connections.

use std::collections::VecDeque;
use std::convert::Infallible;
use std::task::{Context, Poll};
use std::time::Duration;

use libp2p::core::transport::PortUse;
use libp2p::core::Endpoint;
use libp2p::swarm::{
    CloseConnection,
    ConnectionDenied,
    ConnectionHandler,
    ConnectionId,
    FromSwarm,
    NetworkBehaviour,
    ToSwarm,
};
use libp2p::{ping, Multiaddr, PeerId};
use tracing::warn;

/// A behaviour that monitors connection health using ping and disconnects unhealthy connections.
///
/// This behaviour wraps libp2p's ping protocol and immediately disconnects on first ping failure.
/// This prevents split-brain scenarios where one side thinks it's connected while the other
/// doesn't.
///
/// This behaviour is self-contained and does not emit any events. It silently manages
/// connection health in the background.
#[derive(Default)]
pub struct Behaviour {
    ping: ping::Behaviour,
    pending_close_connections: VecDeque<(PeerId, ConnectionId)>,
}

impl Behaviour {
    pub fn new(interval: Duration, timeout: Duration) -> Self {
        let ping_config = ping::Config::new().with_interval(interval).with_timeout(timeout);
        Self {
            ping: ping::Behaviour::new(ping_config),
            pending_close_connections: Default::default(),
        }
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = <ping::Behaviour as NetworkBehaviour>::ConnectionHandler;
    type ToSwarm = Infallible;

    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        self.ping.handle_established_inbound_connection(
            connection_id,
            peer,
            local_addr,
            remote_addr,
        )
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        addr: &Multiaddr,
        role_override: Endpoint,
        port_use: PortUse,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        self.ping.handle_established_outbound_connection(
            connection_id,
            peer,
            addr,
            role_override,
            port_use,
        )
    }

    fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
        self.ping.on_swarm_event(event);
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: <Self::ConnectionHandler as ConnectionHandler>::ToBehaviour,
    ) {
        self.ping.on_connection_handler_event(peer_id, connection_id, event);
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, <Self::ConnectionHandler as ConnectionHandler>::FromBehaviour>>
    {
        loop {
            match self.ping.poll(cx) {
                Poll::Ready(ToSwarm::GenerateEvent(ping_event)) => match ping_event {
                    ping::Event { result: Ok(_), .. } => {}
                    ping::Event { peer, connection, result: Err(failure) } => {
                        warn!(?peer, ?connection, ?failure, "Ping failed, closing connection.");
                        self.pending_close_connections.push_back((peer, connection));
                    }
                },
                Poll::Ready(other) => {
                    unreachable!("Ping behaviour should not generate swarm events: {other:?}.");
                }
                Poll::Pending => break,
            }
        }

        if let Some((peer_id, connection_id)) = self.pending_close_connections.pop_front() {
            return Poll::Ready(ToSwarm::CloseConnection {
                peer_id,
                connection: CloseConnection::One(connection_id),
            });
        }

        Poll::Pending
    }
}
