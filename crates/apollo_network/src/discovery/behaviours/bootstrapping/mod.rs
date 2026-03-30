use std::collections::{HashMap, VecDeque};
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
use tracing::{info, warn};

use crate::discovery::ToOtherBehaviourEvent;

#[cfg(test)]
mod bootstrap_test;

pub struct BootstrappingBehaviour {
    /// Bootstrap peers and their known addresses.
    bootstrap_peers: HashMap<PeerId, Multiaddr>,
    /// Events to emit on the next poll.
    pending_events: VecDeque<ToOtherBehaviourEvent>,
    waker: Option<Waker>,
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
        _port_use: PortUse,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        Ok(dummy::ConnectionHandler)
    }

    fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
        match event {
            FromSwarm::ConnectionEstablished(ConnectionEstablished {
                peer_id,
                other_established: 0,
                ..
            }) => {
                if let Some(address) = self.bootstrap_peers.get(&peer_id) {
                    // Remove any stale RequestDial for this peer that may have been queued
                    // by a prior ConnectionClosed in the same poll cycle.
                    self.pending_events.retain(|event| {
                        !matches!(
                            event,
                            ToOtherBehaviourEvent::RequestDial { peer_id: dial_peer, .. }
                                if *dial_peer == peer_id
                        )
                    });
                    self.pending_events.push_back(ToOtherBehaviourEvent::FoundListenAddresses {
                        peer_id,
                        listen_addresses: vec![address.clone()],
                    });
                    self.wake();
                }
            }
            FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                remaining_established: 0,
                ..
            }) => {
                if let Some(address) = self.bootstrap_peers.get(&peer_id) {
                    self.pending_events.push_back(ToOtherBehaviourEvent::RequestDial {
                        peer_id,
                        addresses: vec![address.clone()],
                    });
                    self.wake();
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
        if let Some(event) = self.pending_events.pop_front() {
            return Poll::Ready(ToSwarm::GenerateEvent(event));
        }
        self.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

impl BootstrappingBehaviour {
    pub fn is_bootstrap_peer(&self, peer_id: &PeerId) -> bool {
        self.bootstrap_peers.contains_key(peer_id)
    }

    pub fn new(local_peer_id: PeerId, bootstrap_peers: Vec<(PeerId, Multiaddr)>) -> Self {
        let unique_peer_ids: std::collections::HashSet<_> =
            bootstrap_peers.iter().map(|(id, _)| id).collect();
        assert!(
            unique_peer_ids.len() == bootstrap_peers.len(),
            "Bootstrap peer IDs must be unique, PeerIds: {bootstrap_peers:?}"
        );

        let mut peers_map = HashMap::new();
        let mut pending_events = VecDeque::new();

        for (peer_id, address) in bootstrap_peers {
            if peer_id == local_peer_id {
                info!("Skipping bootstrap peer with same ID as local peer: {address}");
                continue;
            }
            pending_events.push_back(ToOtherBehaviourEvent::RequestDial {
                peer_id,
                addresses: vec![address.clone()],
            });
            peers_map.insert(peer_id, address);
        }

        if peers_map.is_empty() {
            warn!("No bootstrap peers provided, bootstrapping will not be possible");
        } else {
            info!("Bootstrapping with {} bootstrap peers", peers_map.len());
        }

        Self { bootstrap_peers: peers_map, pending_events, waker: None }
    }

    fn wake(&mut self) {
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
}
