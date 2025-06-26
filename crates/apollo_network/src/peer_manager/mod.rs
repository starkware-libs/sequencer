use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use apollo_config::converters::{
    deserialize_milliseconds_to_duration,
    deserialize_seconds_to_duration,
};
use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use futures::future::BoxFuture;
use futures::FutureExt;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::swarm::{ConnectionId, ToSwarm};
use libp2p::PeerId;
use peer::Peer;
use serde::{Deserialize, Serialize};
use tracing::info;

pub use self::behaviour_impl::ToOtherBehaviourEvent;
use crate::discovery::identify_impl::IdentifyToOtherBehaviourEvent;
use crate::misconduct_score::MisconductScore;
use crate::mixed_behaviour::BridgedBehaviour;
use crate::sqmr::OutboundSessionId;
use crate::{discovery, mixed_behaviour, sqmr};

pub(crate) mod behaviour_impl;
pub(crate) mod peer;
#[cfg(test)]
mod test;

#[cfg_attr(test, derive(Debug, PartialEq))]
#[derive(Clone, Copy)]
pub enum ReputationModifier {
    Misconduct { misconduct_score: MisconductScore },
    Unstable,
}

pub struct PeerManager {
    peers: HashMap<PeerId, Peer>,
    // TODO(Shahak): consider implementing a cleanup mechanism to not store all queries forever
    session_to_peer_map: HashMap<OutboundSessionId, PeerId>,
    config: PeerManagerConfig,
    last_peer_index: usize,
    // TODO(shahak): Change to VecDeque and awake when item is added.
    pending_events: Vec<ToSwarm<ToOtherBehaviourEvent, libp2p::swarm::THandlerInEvent<Self>>>,
    peers_pending_dial_with_sessions: HashMap<PeerId, Vec<OutboundSessionId>>,
    sessions_received_when_no_peers: Vec<OutboundSessionId>,
    sleep_waiting_for_unblocked_peer: Option<BoxFuture<'static, ()>>,
    // A peer is known only after we get the identify message.
    connections_for_unknown_peers: HashMap<PeerId, Vec<ConnectionId>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct PeerManagerConfig {
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    malicious_timeout_seconds: Duration,
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    unstable_timeout_millis: Duration,
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum PeerManagerError {
    #[error("No such peer: {0}")]
    NoSuchPeer(PeerId),
    #[error("No such session: {0}")]
    NoSuchSession(OutboundSessionId),
    #[error("Peer is blocked: {0}")]
    PeerIsBlocked(PeerId),
}

impl Default for PeerManagerConfig {
    fn default() -> Self {
        Self {
            // TODO(shahak): Increase this once we're in a non-trusted setup.
            malicious_timeout_seconds: Duration::from_secs(1),
            unstable_timeout_millis: Duration::from_millis(1000),
        }
    }
}

impl SerializeConfig for PeerManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([
            ser_param(
                "malicious_timeout_seconds",
                &self.malicious_timeout_seconds.as_secs(),
                "The duration in seconds a peer is blacklisted after being marked as malicious.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "unstable_timeout_millis",
                &self.unstable_timeout_millis.as_millis(),
                "The duration in milliseconds a peer blacklisted after being reported as unstable.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

#[allow(dead_code)]
impl PeerManager {
    pub(crate) fn new(config: PeerManagerConfig) -> Self {
        let peers = HashMap::new();
        Self {
            peers,
            session_to_peer_map: HashMap::new(),
            config,
            last_peer_index: 0,
            pending_events: Vec::new(),
            peers_pending_dial_with_sessions: HashMap::new(),
            sessions_received_when_no_peers: Vec::new(),
            sleep_waiting_for_unblocked_peer: None,
            connections_for_unknown_peers: HashMap::default(),
        }
    }

    fn add_peer(&mut self, peer: Peer) {
        info!("NEW_PEER: Peer Manager found new peer {:?}", peer.peer_id());
        self.peers.insert(peer.peer_id(), peer);
        // The new peer is unblocked so we don't need to wait for unblocked peer.
        self.sleep_waiting_for_unblocked_peer = None;
        for outbound_session_id in std::mem::take(&mut self.sessions_received_when_no_peers) {
            self.assign_peer_to_session(outbound_session_id);
        }
    }

    #[cfg(test)]
    fn get_mut_peer(&mut self, peer_id: PeerId) -> Option<&mut Peer> {
        self.peers.get_mut(&peer_id)
    }

    // TODO(shahak): Remove return value and use events in tests.
    // TODO(shahak): Split this function for readability.
    fn assign_peer_to_session(&mut self, outbound_session_id: OutboundSessionId) -> Option<PeerId> {
        // TODO(Shahak): consider moving this logic to be async (on a different tokio task)
        // until then we can return the assignment even if we use events for the notification.
        if self.peers.is_empty() {
            info!("No peers. Waiting for a new peer to be connected for {outbound_session_id:?}");
            self.sessions_received_when_no_peers.push(outbound_session_id);
            return None;
        }
        let peer = self
            .peers
            .iter()
            .skip(self.last_peer_index)
            .find(|(_, peer)| peer.is_available())
            .or_else(|| {
                self.peers.iter().take(self.last_peer_index).find(|(_, peer)| peer.is_available())
            });
        // TODO(shahak): advance to selected peer's index plus one.
        self.last_peer_index = (self.last_peer_index + 1) % self.peers.len();
        if peer.is_none() {
            info!(
                "No unblocked peers with active connection. Waiting for a new peer to be \
                 connected or for a peer to become unblocked and re-discovered for \
                 {outbound_session_id:?}"
            );
            self.sessions_received_when_no_peers.push(outbound_session_id);
            // Find the peer closest to becoming unblocked.
            let sleep_deadline = self
                .peers
                .values()
                .map(|peer| peer.blocked_until())
                .min()
                .expect("min should not return None on a non-empty iterator");
            self.sleep_waiting_for_unblocked_peer =
                Some(tokio::time::sleep_until(sleep_deadline.into()).boxed());
            return None;
        }
        peer.map(|(peer_id, peer)| {
            // TODO(Shahak): consider not allowing reassignment of the same session
            self.session_to_peer_map.insert(outbound_session_id, *peer_id);
            let peer_connection_ids = peer.connection_ids();
            if !peer_connection_ids.is_empty() {
                let connection_id = peer_connection_ids[0];
                self.pending_events.push(ToSwarm::GenerateEvent(
                    ToOtherBehaviourEvent::SessionAssigned {
                        outbound_session_id,
                        peer_id: *peer_id,
                        connection_id,
                    },
                ));
            // TODO(shahak): remove this code block, since we check that peer has connection id
            } else {
                // In case we have a race condition where the connection is closed after we added to
                // the pending list, the reciever will get an error and will need to ask for
                // re-assignment
                if let Some(sessions) = self.peers_pending_dial_with_sessions.get_mut(peer_id) {
                    sessions.push(outbound_session_id);
                } else {
                    self.peers_pending_dial_with_sessions
                        .insert(*peer_id, vec![outbound_session_id]);
                }
                info!("Dialing peer {:?} with multiaddr {:?}", peer_id, peer.multiaddr());
                self.pending_events.push(ToSwarm::Dial {
                    opts: DialOpts::peer_id(*peer_id)
                        .addresses(vec![peer.multiaddr()])
                        // The default condition is Disconnected
                        // TODO(shahak): Solve this instead by adding new peers through
                        // ConnectionEstablished without address.
                        .condition(libp2p::swarm::dial_opts::PeerCondition::Always)
                        .build(),
                });
            }
            *peer_id
        })
    }

    pub(crate) fn report_peer(
        &mut self,
        peer_id: PeerId,
        reason: ReputationModifier,
    ) -> Result<(), PeerManagerError> {
        if let Some(peer) = self.peers.get_mut(&peer_id) {
            match reason {
                ReputationModifier::Misconduct { misconduct_score } => {
                    peer.report(misconduct_score);
                    if peer.is_malicious() {
                        self.pending_events.push(ToSwarm::GenerateEvent(
                            ToOtherBehaviourEvent::PeerBlacklisted { peer_id },
                        ));
                        // TODO(shahak): close the connection with the peer. Do this only when
                        // we're not in a trusted setup.
                        peer.blacklist_peer(self.config.malicious_timeout_seconds);
                        peer.reset_misconduct_score();
                    }
                }
                ReputationModifier::Unstable => {
                    self.pending_events.push(ToSwarm::GenerateEvent(
                        ToOtherBehaviourEvent::PeerBlacklisted { peer_id },
                    ));
                    peer.blacklist_peer(self.config.unstable_timeout_millis);
                }
            }
            Ok(())
        } else {
            Err(PeerManagerError::NoSuchPeer(peer_id))
        }
    }

    fn report_session(
        &mut self,
        outbound_session_id: OutboundSessionId,
        reason: ReputationModifier,
    ) -> Result<(), PeerManagerError> {
        if let Some(peer_id) = self.session_to_peer_map.get(&outbound_session_id) {
            self.report_peer(*peer_id, reason)
        } else {
            Err(PeerManagerError::NoSuchSession(outbound_session_id))
        }
    }
}

impl From<ToOtherBehaviourEvent> for mixed_behaviour::Event {
    fn from(event: ToOtherBehaviourEvent) -> Self {
        Self::ToOtherBehaviourEvent(mixed_behaviour::ToOtherBehaviourEvent::PeerManager(event))
    }
}

impl BridgedBehaviour for PeerManager {
    fn on_other_behaviour_event(&mut self, event: &mixed_behaviour::ToOtherBehaviourEvent) {
        match event {
            mixed_behaviour::ToOtherBehaviourEvent::Sqmr(
                sqmr::ToOtherBehaviourEvent::RequestPeerAssignment { outbound_session_id },
            ) => {
                self.assign_peer_to_session(*outbound_session_id);
            }
            mixed_behaviour::ToOtherBehaviourEvent::Identify(
                IdentifyToOtherBehaviourEvent::FoundListenAddresses { peer_id, listen_addresses },
            )
            | mixed_behaviour::ToOtherBehaviourEvent::Discovery(
                discovery::ToOtherBehaviourEvent::FoundListenAddresses {
                    peer_id,
                    listen_addresses,
                },
            ) => {
                // TODO(shahak): Handle changed addresses
                if self.peers.contains_key(peer_id) {
                    return;
                }
                // TODO(shahak): Track multiple addresses per peer.
                let Some(address) = listen_addresses.first() else {
                    return;
                };

                let mut peer = Peer::new(*peer_id, address.clone());
                if let Some(connection_ids) = self.connections_for_unknown_peers.remove(peer_id) {
                    *peer.connection_ids_mut() = connection_ids;
                }
                self.add_peer(peer);
            }
            _ => {}
        }
    }
}
