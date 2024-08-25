use std::time::Duration;

use libp2p::swarm::ConnectionId;
use libp2p::{Multiaddr, PeerId};
use tokio::time::Instant;
use tracing::info;

pub trait PeerTrait {
    fn new(peer_id: PeerId, multiaddr: Multiaddr) -> Self;

    fn update_reputation(&mut self, timeout_duration: Duration);

    fn peer_id(&self) -> PeerId;

    fn multiaddr(&self) -> Multiaddr;

    fn is_blocked(&self) -> bool;

    /// Returns Instant::now if not blocked.
    fn blocked_until(&self) -> Instant;

    fn connection_ids(&self) -> &Vec<ConnectionId>;

    fn add_connection_id(&mut self, connection_id: ConnectionId);

    fn remove_connection_id(&mut self, connection_id: ConnectionId);
}

#[derive(Clone)]
pub struct Peer {
    peer_id: PeerId,
    multiaddr: Multiaddr,
    timed_out_until: Instant,
    connection_ids: Vec<ConnectionId>,
}

impl PeerTrait for Peer {
    fn new(peer_id: PeerId, multiaddr: Multiaddr) -> Self {
        Self { peer_id, multiaddr, timed_out_until: Instant::now(), connection_ids: Vec::new() }
    }

    fn update_reputation(&mut self, timeout_duration: Duration) {
        self.timed_out_until = Instant::now() + timeout_duration;
        info!(
            "Peer {:?} misbehaved. Blacklisting it for {:.3} seconds.",
            self.peer_id,
            timeout_duration.as_secs_f64(),
        );
    }

    fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    fn multiaddr(&self) -> Multiaddr {
        self.multiaddr.clone()
    }

    fn is_blocked(&self) -> bool {
        self.timed_out_until > Instant::now()
    }

    fn blocked_until(&self) -> Instant {
        if self.timed_out_until > Instant::now() { self.timed_out_until } else { Instant::now() }
        // self.timed_out_until.unwrap_or_else(Instant::now)
    }

    fn connection_ids(&self) -> &Vec<ConnectionId> {
        &self.connection_ids
    }

    fn add_connection_id(&mut self, connection_id: ConnectionId) {
        self.connection_ids.push(connection_id);
    }

    fn remove_connection_id(&mut self, connection_id: ConnectionId) {
        self.connection_ids.retain(|&id| id != connection_id);
    }
}
