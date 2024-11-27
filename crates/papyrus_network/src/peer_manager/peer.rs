use std::time::{Duration, Instant};

use libp2p::swarm::ConnectionId;
use libp2p::{Multiaddr, PeerId};
use tracing::info;

pub trait PeerTrait {
    fn new(peer_id: PeerId, multiaddr: Multiaddr) -> Self;

    fn blacklist_peer(&mut self, timeout_duration: Duration);

    fn peer_id(&self) -> PeerId;

    fn multiaddr(&self) -> Multiaddr;

    fn is_blocked(&self) -> bool;

    /// Returns Instant::now if not blocked.
    fn blocked_until(&self) -> Instant;

    fn connection_ids(&self) -> &Vec<ConnectionId>;

    fn add_connection_id(&mut self, connection_id: ConnectionId);

    fn remove_connection_id(&mut self, connection_id: ConnectionId);

    fn reset_misconduct_score(&mut self);

    fn report(&mut self, misconduct_score: f64);

    fn is_malicious(&self) -> bool;
}

#[derive(Clone)]
pub struct Peer {
    peer_id: PeerId,
    multiaddr: Multiaddr,
    timed_out_until: Instant,
    connection_ids: Vec<ConnectionId>,
    misconduct_score: f64,
}

impl PeerTrait for Peer {
    fn new(peer_id: PeerId, multiaddr: Multiaddr) -> Self {
        Self {
            peer_id,
            multiaddr,
            timed_out_until: get_instant_now(),
            connection_ids: Vec::new(),
            misconduct_score: 0f64,
        }
    }

    fn blacklist_peer(&mut self, timeout_duration: Duration) {
        self.timed_out_until = get_instant_now() + timeout_duration;
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
        self.timed_out_until > get_instant_now()
    }

    fn blocked_until(&self) -> Instant {
        if self.timed_out_until > get_instant_now() {
            self.timed_out_until
        } else {
            get_instant_now()
        }
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

    fn reset_misconduct_score(&mut self) {
        self.misconduct_score = 0f64;
    }

    fn report(&mut self, misconduct_score: f64) {
        self.misconduct_score += misconduct_score;
    }

    fn is_malicious(&self) -> bool {
        1.0f64 <= self.misconduct_score
    }
}

#[cfg(not(test))]
fn get_instant_now() -> Instant {
    Instant::now()
}

// In tests we simulate time passing using tokio, so we need to use tokio's Instant instead of std.
#[cfg(test)]
fn get_instant_now() -> Instant {
    tokio::time::Instant::now().into_std()
}
