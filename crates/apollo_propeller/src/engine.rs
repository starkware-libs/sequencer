//! Propeller engine logic.
//!
//! This module contains the protocol logic (broadcasting, validation, reconstruction, committee
//! management). The engine runs as an async task and communicates with the libp2p
//! `NetworkBehaviour` adapter in `behaviour.rs` via command/output channels.

use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::sync::Arc;

use apollo_infra_utils::warn_every_n_ms;
use libp2p::identity::{Keypair, PeerId, PublicKey};
use lru::LruCache;
use starknet_api::staking::StakingWeight;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, trace, warn};

use crate::config::Config;
use crate::handler::{HandlerIn, HandlerOut};
use crate::message_processor::{EventStateManagerToEngine, MessageProcessor, UnitToValidate};
use crate::metrics::PropellerMetrics;
use crate::sharding::create_units_to_publish;
use crate::signature;
use crate::time_cache::TimeCache;
use crate::tree::PropellerScheduleManager;
use crate::types::{CommitteeId, CommitteeSetupError, Event, MessageRoot, ShardPublishError};
use crate::unit::PropellerUnit;

#[cfg(test)]
#[path = "engine_test.rs"]
mod engine_test;

// TODO(guyn): move this to the propeller Config.
// Must be much bigger than the number of peers we expect to work with (2*committee_size).
const PEER_NONCE_CACHE_SIZE: usize = 1000;

type BroadcastResponseTx = oneshot::Sender<Result<(), ShardPublishError>>;
type BroadcastResult = (Result<Vec<PropellerUnit>, ShardPublishError>, BroadcastResponseTx);

// TODO(guyn): since the nonce is part of the MessageKey, there's a risk that getting a bad nonce
// will cause a message processor to start working on a bad message, which later takes up resources
// or blocks legitimate messages. To prevent this we must make sure that message processors that
// never get a correct signature (with the right nonce) are not counted in the
// messages_to_ignore_shards_from cache, and don't update the nonce tracked for each peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct MessageKey {
    committee_id: CommitteeId,
    publisher: PeerId,
    nonce: u64,
    root: MessageRoot,
}

/// Commands sent from Behaviour to Engine.
pub enum EngineCommand {
    RegisterCommitteePeers {
        committee_id: CommitteeId,
        peers: Vec<(PeerId, StakingWeight, Option<PublicKey>)>,
        response: oneshot::Sender<Result<(), CommitteeSetupError>>,
    },
    // TODO(AndrewL): remove this variant once unregister is no longer needed.
    UnregisterCommittee {
        committee_id: CommitteeId,
        response: oneshot::Sender<bool>,
    },
    Broadcast {
        committee_id: CommitteeId,
        message: Vec<u8>,
        response_tx: BroadcastResponseTx,
    },
    HandleHandlerOutput {
        peer_id: PeerId,
        output: HandlerOut,
    },
    HandleConnected {
        peer_id: PeerId,
    },
    HandleDisconnected {
        peer_id: PeerId,
    },
}

/// Outputs sent from the engine to the Behaviour.
pub enum EngineOutput {
    GenerateEvent(Event),
    NotifyHandler { peer_id: PeerId, event: HandlerIn },
}

/// Data associated with a single committee.
struct CommitteeData {
    schedule_manager: Arc<PropellerScheduleManager>,
    peer_public_keys: HashMap<PeerId, PublicKey>,
}

/// The Propeller engine, run as an async task via [`Engine::run`].
pub struct Engine {
    config: Config,
    committees: HashMap<CommitteeId, CommitteeData>,
    connected_peers: HashSet<PeerId>,
    keypair: Keypair,
    local_peer_id: PeerId,
    // TODO(AndrewL): limit the number of message processors per publisher to avoid resource
    // exhaustion.
    /// Registry of per-message unit senders to their message processors.
    message_to_unit_tx: HashMap<MessageKey, mpsc::UnboundedSender<UnitToValidate>>,
    /// Messages that have already been encountered and processed (for deduplication).
    messages_to_ignore_shards_from: TimeCache<MessageKey>,
    // TODO(guyn): track nonces separately for each committee.
    /// Nonce per peer. LRU cache is used as a passive garbage collection mechanism.
    peer_nonce: LruCache<PeerId, u64>,
    /// Receiver for messages from state manager tasks.
    state_manager_rx: mpsc::UnboundedReceiver<EventStateManagerToEngine>,
    state_manager_tx: mpsc::UnboundedSender<EventStateManagerToEngine>,
    prepared_units_rx: mpsc::UnboundedReceiver<BroadcastResult>,
    prepared_units_tx: mpsc::UnboundedSender<BroadcastResult>,
    from_behaviour_rx: mpsc::UnboundedReceiver<EngineCommand>,
    to_behaviour_tx: mpsc::UnboundedSender<EngineOutput>,
    metrics: Option<PropellerMetrics>,
}

impl Engine {
    /// Create a new engine instance.
    pub fn new(
        keypair: Keypair,
        config: Config,
        from_behaviour_rx: mpsc::UnboundedReceiver<EngineCommand>,
        output_tx: mpsc::UnboundedSender<EngineOutput>,
        metrics: Option<PropellerMetrics>,
    ) -> Self {
        let local_peer_id = PeerId::from(keypair.public());
        let (state_manager_tx, state_manager_rx) = mpsc::unbounded_channel();
        let (broadcaster_results_tx, broadcaster_results_rx) = mpsc::unbounded_channel();

        let messages_to_ignore_shards_from = TimeCache::new(config.stale_message_timeout);

        Self {
            committees: HashMap::new(),
            config,
            connected_peers: HashSet::new(),
            keypair,
            local_peer_id,
            message_to_unit_tx: HashMap::new(),
            messages_to_ignore_shards_from,
            peer_nonce: LruCache::new(
                NonZeroUsize::new(PEER_NONCE_CACHE_SIZE).expect("Cache size must be non-zero"),
            ),
            state_manager_rx,
            state_manager_tx,
            prepared_units_rx: broadcaster_results_rx,
            prepared_units_tx: broadcaster_results_tx,
            from_behaviour_rx,
            to_behaviour_tx: output_tx,
            metrics,
        }
    }

    // TODO(AndrewL): create a Committee struct wrapping a Vec<CommitteeMember> and a
    // CommitteeMember struct for (PeerId, Stake, Option<PublicKey>).
    /// Register a committee with peers and optional public keys.
    pub fn register_committee(
        &mut self,
        committee_id: CommitteeId,
        peers: Vec<(PeerId, StakingWeight, Option<PublicKey>)>,
    ) -> Result<(), CommitteeSetupError> {
        if self.committees.contains_key(&committee_id) {
            warn!(?committee_id, "Committee already registered, ignoring re-registration");
            return Ok(());
        }

        let mut peer_weights = Vec::new();
        let mut peer_public_keys = HashMap::new();

        for (peer_id, weight, public_key) in peers {
            let public_key = self.get_public_key(peer_id, public_key)?;
            peer_weights.push((peer_id, weight));
            peer_public_keys.insert(peer_id, public_key);
        }

        let schedule_manager = PropellerScheduleManager::new(self.local_peer_id, peer_weights)?;
        let committee_data =
            CommitteeData { schedule_manager: Arc::new(schedule_manager), peer_public_keys };
        self.committees.insert(committee_id, committee_data);

        Ok(())
    }

    /// Unregister a committee.
    // TODO(AndrewL): clean up message_to_unit_tx entries and terminate their processor tasks on
    // unregister to avoid resource leaks.
    pub fn unregister_committee(&mut self, committee_id: CommitteeId) -> bool {
        let result = self.committees.remove(&committee_id).is_some();
        // TODO(AndrewL): Consider adding a command to MP to terminate the task.
        self.message_to_unit_tx.retain(|key, _| key.committee_id != committee_id);
        result
    }

    /// TODO(AndrewL): document this.
    fn prepare_units(
        &mut self,
        committee_id: CommitteeId,
        message: Vec<u8>,
        response_tx: BroadcastResponseTx,
    ) {
        let Some(schedule_manager) =
            self.committees.get(&committee_id).map(|data| data.schedule_manager.clone())
        else {
            let _ = response_tx.send(Err(ShardPublishError::CommitteeNotRegistered(committee_id)));
            return;
        };

        let keypair = self.keypair.clone();
        let num_data_shards = schedule_manager.num_data_shards();
        let num_coding_shards = schedule_manager.num_coding_shards();
        let prepared_units_tx_clone = self.prepared_units_tx.clone();

        tokio::task::spawn_blocking(move || {
            let result = create_units_to_publish(
                message,
                committee_id,
                keypair,
                num_data_shards,
                num_coding_shards,
            );
            let _ = prepared_units_tx_clone.send((result, response_tx));
        });
    }

    /// Handle a peer connection.
    fn handle_connected(&mut self, peer_id: PeerId) {
        self.connected_peers.insert(peer_id);
    }

    /// Handle a peer disconnection.
    fn handle_disconnected(&mut self, peer_id: PeerId) {
        self.connected_peers.remove(&peer_id);
    }

    /// Handle an incoming unit from a peer.
    fn handle_unit(&mut self, sender_peer_id: PeerId, unit: PropellerUnit) {
        let claimed_committee_id = unit.committee_id();
        let claimed_publisher = unit.publisher();
        let claimed_nonce = unit.nonce();
        let claimed_root = unit.root();

        // Track received unit.
        if let Some(metrics) = &self.metrics {
            metrics.units_received.increment(1);
        }

        // Check if committee is registered.
        let Some(committee_data) = self.committees.get(&claimed_committee_id) else {
            warn!(?claimed_committee_id, "Received unit for unregistered committee, dropping");
            return;
        };

        // Skip if message already finalized.
        let message_key = MessageKey {
            committee_id: claimed_committee_id,
            publisher: claimed_publisher,
            nonce: claimed_nonce,
            root: claimed_root,
        };

        if self.messages_to_ignore_shards_from.contains(&message_key) {
            trace!(?message_key, "Message already finalized, dropping unit");
            return;
        }

        // Spawn tasks if this is a new message.
        if !self.message_to_unit_tx.contains_key(&message_key) {
            let nonce = self.peer_nonce.get(&claimed_publisher).copied().unwrap_or(0);
            if nonce >= claimed_nonce {
                warn_every_n_ms!(
                    2000,
                    "Message nonce is too old, dropping unit to prevent replay attacks"
                );
                return;
            }

            debug!(?message_key, "[ENGINE] Spawning new message processor");

            let schedule_manager = committee_data.schedule_manager.clone();
            let Some(publisher_public_key) =
                committee_data.peer_public_keys.get(&claimed_publisher).cloned()
            else {
                warn!(?claimed_publisher, "Received unit for unregistered publisher, dropping");
                return;
            };
            let my_shard_index_result =
                schedule_manager.get_my_shard_index_given_publisher(&claimed_publisher);
            let Ok(my_shard_index) = my_shard_index_result else {
                warn!(
                    ?claimed_publisher,
                    ?claimed_committee_id,
                    ?my_shard_index_result,
                    "Received unit for publisher not in committee, dropping"
                );
                return;
            };

            // Create channel for Engine -> MessageProcessor communication
            let (unit_tx, unit_rx) = mpsc::unbounded_channel();

            // Create and spawn message processor
            let processor = MessageProcessor {
                committee_id: claimed_committee_id,
                publisher: claimed_publisher,
                nonce: claimed_nonce,
                message_root: claimed_root,
                my_shard_index,
                publisher_public_key,
                tree_manager: Arc::clone(&schedule_manager),
                local_peer_id: self.local_peer_id,
                unit_rx,
                engine_tx: self.state_manager_tx.clone(),
                timeout: self.config.stale_message_timeout,
            };

            // TODO(AndrewL): track task handle to see if it panics or is killed.
            // TODO(AndrewL): abort the task if committee is removed.
            tokio::spawn(processor.run());

            self.message_to_unit_tx.insert(message_key, unit_tx);
        }

        // Send unit to message processor
        let unit_tx =
            self.message_to_unit_tx.get(&message_key).expect("Message processor must exist");

        // This may fail if the message is already finalized
        let _ = unit_tx.send((sender_peer_id, unit));
    }

    /// Handle a send error from the handler.
    fn handle_send_error(&mut self, peer_id: PeerId, error: String) {
        // TODO(AndrewL): Consider a re-try mechanism.
        self.emit_event(Event::ShardSendFailed {
            sent_from: None,
            sent_to: Some(peer_id),
            error: ShardPublishError::HandlerError(error),
        });
    }

    fn emit_event(&mut self, event: Event) {
        self.to_behaviour_tx
            .send(EngineOutput::GenerateEvent(event))
            .expect("Behaviour has exited");
    }

    // TODO(AndrewL): consider working with ConnectionId instead of PeerId here.
    fn emit_handler_event(&mut self, peer_id: PeerId, event: HandlerIn) {
        if !self.connected_peers.contains(&peer_id) {
            self.emit_event(Event::ShardSendFailed {
                sent_from: None,
                sent_to: Some(peer_id),
                error: ShardPublishError::NotConnectedToPeer(peer_id),
            });
            return;
        }
        self.to_behaviour_tx
            .send(EngineOutput::NotifyHandler { peer_id, event })
            .expect("Behaviour has exited");
    }

    fn handle_broadcaster_result(
        &mut self,
        result: Result<Vec<PropellerUnit>, ShardPublishError>,
        response: BroadcastResponseTx,
    ) {
        if let Err(error) = result.and_then(|units| self.broadcast_prepared_units(units)) {
            let _ = response.send(Err(error));
        } else {
            let _ = response.send(Ok(()));
        }
    }

    /// Handle messages from state manager tasks.
    fn handle_state_manager_message(&mut self, msg: EventStateManagerToEngine) {
        match msg {
            EventStateManagerToEngine::BehaviourEvent(event) => {
                self.emit_event(event);
            }
            EventStateManagerToEngine::Finalized {
                committee_id,
                publisher,
                nonce,
                message_root,
            } => {
                trace!(
                    ?committee_id,
                    ?publisher,
                    ?nonce,
                    ?message_root,
                    "[ENGINE] Message finalized"
                );

                // Mark as finalized
                let message_key = MessageKey { committee_id, publisher, nonce, root: message_root };
                let expired_keys =
                    self.messages_to_ignore_shards_from.insert_and_get_expired(message_key);

                if !expired_keys.is_empty() {
                    trace!(?expired_keys, "[ENGINE] Removed expired messages from TTL cache");
                    for key in expired_keys {
                        // Update the nonce to the latest timestamp if it is bigger.
                        let new_nonce = self
                            .peer_nonce
                            .peek(&key.publisher)
                            .copied()
                            .unwrap_or(0)
                            .max(key.nonce);
                        self.peer_nonce.put(key.publisher, new_nonce);
                    }
                }

                // Clean up task handles
                if self.message_to_unit_tx.remove(&message_key).is_some() {
                    trace!(?message_key, "[ENGINE] Removed task handles");
                }
            }
            EventStateManagerToEngine::SendUnitToPeers { unit, peers } => {
                trace!(index = ?unit.index(), num_peers = peers.len(), "[ENGINE] Forwarding unit to peers");

                for peer in peers {
                    self.send_unit_to_peer(unit.clone(), peer);
                }
            }
        }
    }

    // TODO(AndrewL): remove this once we have StakerID instead of PeerId.
    fn get_public_key(
        &self,
        peer_id: PeerId,
        public_key: Option<PublicKey>,
    ) -> Result<PublicKey, CommitteeSetupError> {
        match public_key {
            Some(pk) => {
                if signature::validate_public_key_matches_peer_id(&pk, &peer_id) {
                    Ok(pk)
                } else {
                    Err(CommitteeSetupError::InvalidPublicKey)
                }
            }
            None => signature::try_extract_public_key_from_peer_id(&peer_id)
                .ok_or(CommitteeSetupError::InvalidPublicKey),
        }
    }

    fn broadcast_prepared_units(
        &mut self,
        units: Vec<PropellerUnit>,
    ) -> Result<(), ShardPublishError> {
        let Some(first_unit) = units.first() else {
            return Ok(());
        };
        let committee_id = first_unit.committee_id();
        trace!(publisher = ?self.local_peer_id, num_units = units.len(), "[BROADCAST] Broadcasting units");

        let schedule_manager = self
            .committees
            .get(&committee_id)
            .ok_or(ShardPublishError::CommitteeNotRegistered(committee_id))?
            .schedule_manager
            .clone();

        let peers_in_order = schedule_manager.make_broadcast_list();
        assert_eq!(
            peers_in_order.len(),
            units.len(),
            "Number of units and peers in order must match"
        );

        for (unit, peer) in units.into_iter().zip(peers_in_order) {
            trace!(index = ?unit.index(), ?peer, "[BROADCAST] Sending unit");
            self.send_unit_to_peer(unit, peer);
        }

        Ok(())
    }

    fn send_unit_to_peer(&mut self, unit: PropellerUnit, peer: PeerId) {
        self.emit_handler_event(peer, HandlerIn::SendUnit(unit));
    }

    /// Run the engine in its own task, processing commands and results.
    pub async fn run(mut self) {
        loop {
            tokio::select! {
                Some(cmd) = self.from_behaviour_rx.recv() => match cmd {
                    EngineCommand::RegisterCommitteePeers { committee_id, peers, response } => {
                        let result = self.register_committee(committee_id, peers);
                        let _ = response.send(result);
                    }
                    EngineCommand::UnregisterCommittee { committee_id, response } => {
                        let _ = response.send(self.unregister_committee(committee_id));
                    }
                    EngineCommand::Broadcast { committee_id, message, response_tx } => {
                        self.prepare_units(committee_id, message, response_tx);
                    }
                    EngineCommand::HandleHandlerOutput { peer_id, output } => match output {
                        HandlerOut::Unit(unit) => self.handle_unit(peer_id, unit),
                        HandlerOut::SendError(error) => self.handle_send_error(peer_id, error),
                    },
                    EngineCommand::HandleConnected { peer_id } => self.handle_connected(peer_id),
                    EngineCommand::HandleDisconnected { peer_id } => self.handle_disconnected(peer_id),
                },

                Some((result, response)) = self.prepared_units_rx.recv() => {
                    self.handle_broadcaster_result(result, response);
                }

                Some(msg) = self.state_manager_rx.recv() => {
                    self.handle_state_manager_message(msg);
                }

                else => {
                    error!("Engine task shutting down");
                    break;
                }
            }
        }
    }
}
