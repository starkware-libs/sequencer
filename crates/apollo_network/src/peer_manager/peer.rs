use std::time::{Duration, Instant};

use libp2p::swarm::ConnectionId;
use libp2p::{Multiaddr, PeerId};
use tracing::info;

use crate::misconduct_score::MisconductScore;

#[derive(Clone)]
pub struct Peer {
    peer_id: PeerId,
    multiaddr: Multiaddr,
    timed_out_until: Instant,
    connection_ids: Vec<ConnectionId>,
    misconduct_score: MisconductScore,
}

impl Peer {
    pub fn new(peer_id: PeerId, multiaddr: Multiaddr) -> Self {
        Self {
            peer_id,
            multiaddr,
            timed_out_until: get_instant_now(),
            connection_ids: Vec::new(),
            misconduct_score: MisconductScore::NEUTRAL,
        }
    }

    pub fn blacklist_peer(&mut self, timeout_duration: Duration) {
        self.timed_out_until = get_instant_now() + timeout_duration;
        info!(
            "Peer {:?} misbehaved. Blacklisting it for {:.3} seconds.",
            self.peer_id,
            timeout_duration.as_secs_f64(),
        );
    }

    pub fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    pub fn multiaddr(&self) -> Multiaddr {
        self.multiaddr.clone()
    }

    pub fn is_blocked(&self) -> bool {
        self.timed_out_until > get_instant_now()
    }

    pub fn is_available(&self) -> bool {
        (!self.is_blocked()) && (!self.connection_ids.is_empty())
    }

    pub fn blocked_until(&self) -> Instant {
        if self.timed_out_until > get_instant_now() {
            self.timed_out_until
        } else {
            get_instant_now()
        }
    }

    pub fn connection_ids(&self) -> &Vec<ConnectionId> {
        &self.connection_ids
    }

    pub fn connection_ids_mut(&mut self) -> &mut Vec<ConnectionId> {
        &mut self.connection_ids
    }

    pub fn add_connection_id(&mut self, connection_id: ConnectionId) {
        self.connection_ids.push(connection_id);
    }

    pub fn reset_misconduct_score(&mut self) {
        self.misconduct_score = MisconductScore::NEUTRAL;
    }

    pub fn report(&mut self, misconduct_score: MisconductScore) {
        self.misconduct_score += misconduct_score;
    }

    pub fn is_malicious(&self) -> bool {
        self.misconduct_score.is_malicious()
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
