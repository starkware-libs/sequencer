use std::task::{Context, Poll};

use libp2p::core::Endpoint;
use libp2p::swarm::behaviour::ConnectionEstablished;
use libp2p::swarm::dial_opts::{DialOpts, PeerCondition};
use libp2p::swarm::{
    dummy,
    AddressChange,
    ConnectionClosed,
    ConnectionDenied,
    ConnectionHandler,
    ConnectionId,
    DialFailure,
    FromSwarm,
    NetworkBehaviour,
    ToSwarm,
};
use libp2p::{Multiaddr, PeerId};
use tokio::time::Instant;
use tokio_retry::strategy::ExponentialBackoff;

use crate::discovery::{RetryConfig, ToOtherBehaviourEvent};

pub struct BootstrappingBehaviour {
    bootstrap_dial_retry_config: RetryConfig,
    bootstrap_peer_address: Multiaddr,
    bootstrap_peer_id: PeerId,
    is_dialing_to_bootstrap_peer: bool,
    is_connected_to_bootstrap_peer: bool,
    is_bootstrap_in_kad_routing_table: bool,
    bootstrap_dial_retry_strategy: ExponentialBackoff,
    time_for_next_bootstrap_dial: Instant,
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
        match event {
            FromSwarm::DialFailure(DialFailure { peer_id: Some(peer_id), .. })
                if peer_id == self.bootstrap_peer_id =>
            {
                self.is_dialing_to_bootstrap_peer = false;
                // For the case that the reason for failure is consistent (e.g the bootstrap peer
                // is down), we sleep before redialing
                self.time_for_next_bootstrap_dial = tokio::time::Instant::now()
                    + self
                        .bootstrap_dial_retry_strategy
                        .next()
                        .expect("Dial sleep strategy ended even though it's an infinite iterator.");
            }
            FromSwarm::ConnectionEstablished(ConnectionEstablished { peer_id, .. })
                if peer_id == self.bootstrap_peer_id =>
            {
                self.is_connected_to_bootstrap_peer = true;
                self.is_dialing_to_bootstrap_peer = false;
                self.bootstrap_dial_retry_strategy = self.bootstrap_dial_retry_config.strategy();
            }
            FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                remaining_established,
                ..
            }) if peer_id == self.bootstrap_peer_id && remaining_established == 0 => {
                self.is_connected_to_bootstrap_peer = false;
                self.is_dialing_to_bootstrap_peer = false;
                self.is_bootstrap_in_kad_routing_table = false;
            }
            FromSwarm::AddressChange(AddressChange { peer_id, .. })
                if peer_id == self.bootstrap_peer_id =>
            {
                todo!();
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
        let is_connected_to_bootstrap_peer = self.is_connected_to_bootstrap_peer;
        let is_bootstrap_in_kad_routing_table = self.is_bootstrap_in_kad_routing_table;
        let bootstrap_peer_id = self.bootstrap_peer_id;
        let bootstrap_peer_address = self.bootstrap_peer_address.clone();
        let time_for_next_bootstrap_dial = self.time_for_next_bootstrap_dial;
        let is_dialing_to_bootstrap_peer = self.is_dialing_to_bootstrap_peer;
        let now = tokio::time::Instant::now();

        if is_connected_to_bootstrap_peer && !is_bootstrap_in_kad_routing_table {
            self.is_bootstrap_in_kad_routing_table = true;

            return Poll::Ready(ToSwarm::GenerateEvent(
                ToOtherBehaviourEvent::FoundListenAddresses {
                    peer_id: bootstrap_peer_id,
                    listen_addresses: vec![bootstrap_peer_address],
                },
            ));
        }

        // TODO(Shahak): If one of the last two conditions is false, register a waker and
        // wake it when we receive an event that we've disconnected from the bootstrap peer.
        // (Right now, when we're disconnected from the bootstrap peer, we'll wait for next
        // kad query even if time_for_next_bootstrap_dial is smaller than
        // time_for_next_kad_query)
        // No need to perform a dial if there's an active dial attempt or we're already
        // connected.

        if !(is_dialing_to_bootstrap_peer)
            && !(is_connected_to_bootstrap_peer)
            && (time_for_next_bootstrap_dial <= now)
        {
            self.is_dialing_to_bootstrap_peer = true;
            Poll::Ready(ToSwarm::Dial {
                opts: DialOpts::peer_id(bootstrap_peer_id)
                        .addresses(vec![bootstrap_peer_address])
                        // The peer manager might also be dialing to the bootstrap node.
                        .condition(PeerCondition::DisconnectedAndNotDialing)
                        .build(),
            })
        } else {
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

impl BootstrappingBehaviour {
    pub fn new(
        bootstrap_dial_retry_config: RetryConfig,
        bootstrap_peer_id: PeerId,
        bootstrap_peer_address: Multiaddr,
    ) -> Self {
        let bootstrap_dial_retry_strategy = bootstrap_dial_retry_config.strategy();
        Self {
            bootstrap_dial_retry_config,
            bootstrap_peer_id,
            bootstrap_peer_address,
            is_dialing_to_bootstrap_peer: false,
            is_connected_to_bootstrap_peer: false,
            is_bootstrap_in_kad_routing_table: false,
            bootstrap_dial_retry_strategy,
            time_for_next_bootstrap_dial: tokio::time::Instant::now(),
        }
    }

    #[cfg(test)]
    pub fn bootstrap_peer_id(&self) -> PeerId {
        self.bootstrap_peer_id
    }

    #[cfg(test)]
    pub fn bootstrap_peer_address(&self) -> &Multiaddr {
        &self.bootstrap_peer_address
    }
}
