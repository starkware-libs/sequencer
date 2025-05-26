use std::task::{Context, Poll};

use bootstrap_peer::BootstrapPeer;
use futures::stream::SelectAll;
use futures::StreamExt;
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
use tracing::info;

use crate::discovery::{RetryConfig, ToOtherBehaviourEvent};

pub mod bootstrap_peer;
#[cfg(test)]
mod bootstrap_test;

pub struct BootstrappingBehaviour {
    peers: SelectAll<BootstrapPeer>,
}

impl NetworkBehaviour for BootstrappingBehaviour {
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

    fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
        for peer in self.peers.iter_mut() {
            peer.on_swarm_event(event);
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
        self.peers.poll_next_unpin(cx).map(|e| e.unwrap())
    }
}

impl BootstrappingBehaviour {
    pub fn new(
        local_peer_id: PeerId,
        bootstrap_dial_retry_config: RetryConfig,
        bootstrap_peers: Vec<(PeerId, Multiaddr)>,
    ) -> Self {
        let mut peers = SelectAll::new();
        for (bootstrap_peer_id, bootstrap_peer_address) in bootstrap_peers {
            if bootstrap_peer_id == local_peer_id {
                info!(
                    "Skipping bootstrap peer with same ID as local peer: {bootstrap_peer_address}"
                );
                continue;
            }
            peers.push(BootstrapPeer::new(
                bootstrap_dial_retry_config,
                bootstrap_peer_id,
                bootstrap_peer_address,
            ));
        }
        Self { peers }
    }
}
