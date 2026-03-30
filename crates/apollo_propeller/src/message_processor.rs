use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;

use libp2p::identity::{PeerId, PublicKey};
use rand::seq::SliceRandom;
use tokio::sync::mpsc;
use tracing::{debug, error, trace};

use crate::sharding::reconstruct_data_shards;
use crate::tree::PropellerScheduleManager;
use crate::types::{
    CommitteeId,
    Event,
    MessageRoot,
    ReconstructionError,
    UnitValidationError,
    VerifiedFields,
};
use crate::unit::{PropellerUnit, ShardsOfPeer};
use crate::unit_validator::UnitValidator;
use crate::{MerkleProof, UnitIndex};

pub type UnitToValidate = (PeerId, PropellerUnit);
type ValidationResult = (Result<(), UnitValidationError>, UnitValidator, PropellerUnit);
type ReconstructionResult = Result<ReconstructionOutput, ReconstructionError>;

#[derive(Debug)]
pub enum EventStateManagerToEngine {
    BehaviourEvent(Event),
    Finalized {
        committee_id: CommitteeId,
        publisher: PeerId,
        nonce: u64,
        message_root: MessageRoot,
    },
    SendUnitToPeers {
        unit: PropellerUnit,
        peers: Vec<PeerId>,
    },
}

#[derive(Debug)]
struct ReconstructionOutput {
    message: Vec<u8>,
    my_shards: ShardsOfPeer,
    my_shard_proof: MerkleProof,
}

enum AddUnitAction {
    NoOp,
    Reconstruct(Vec<PropellerUnit>),
    Emit(Vec<u8>),
}

/// Tracks reconstruction progress for a single message.
enum ReconstructionState {
    PreConstruction {
        received_units: Vec<PropellerUnit>,
        did_broadcast_my_unit: bool,
        verified_fields: Option<VerifiedFields>,
    },
    /// Message was reconstructed but not yet delivered to the application. We keep collecting
    /// units until the emit threshold is reached, then emit the message.
    // No need to track the unit indices after reconstruction (unit duplication already validated)
    PostConstruction { reconstructed_message: Option<Vec<u8>>, num_held_units: usize },
}

impl ReconstructionState {
    fn new() -> Self {
        Self::PreConstruction {
            received_units: Vec::new(),
            did_broadcast_my_unit: false,
            verified_fields: None,
        }
    }

    fn did_broadcast_my_unit(&self) -> bool {
        match self {
            Self::PreConstruction { did_broadcast_my_unit, .. } => *did_broadcast_my_unit,
            Self::PostConstruction { .. } => true,
        }
    }

    /// Absorbs a validated unit into the state and returns the next action to take.
    fn add_unit(
        &mut self,
        unit: PropellerUnit,
<<<<<<< HEAD
        my_shard_index: UnitIndex,
=======
        my_unit_index: UnitIndex,
>>>>>>> 0619ce9469 (apollo_propeller: fix engine.rs comments to use 'unit' instead of 'shard' when referring to PropellerUnit)
        tree_manager: &PropellerScheduleManager,
    ) -> AddUnitAction {
<<<<<<< HEAD
        let is_my_shard = unit.index() == my_unit_index;

        match self {
            Self::PreConstruction { received_units, did_broadcast_my_unit, verified_fields } => {
                if is_my_shard {
=======
        let is_my_unit = unit.index() == my_unit_index;

        match self {
            Self::PreConstruction { received_units, did_broadcast_my_unit, verified_fields } => {
                if is_my_unit {
>>>>>>> a52d5e050a (apollo_l1_events: replace panic with retry in CatchUpper spawned task (#13328))
                    *did_broadcast_my_unit = true;
                }
                if verified_fields.is_none() {
                    *verified_fields = Some(VerifiedFields {
                        signature: unit.signature().to_vec(),
                        nonce: unit.nonce(),
                    });
                }
                received_units.push(unit);
                if tree_manager.should_build(received_units.len()) {
                    AddUnitAction::Reconstruct(std::mem::take(received_units))
                } else {
                    AddUnitAction::NoOp
                }
            }
            // During reconstruction we broadcast our unit, so receiving it back from the
            // network should not inflate the count.
            Self::PostConstruction { num_held_units, .. } => {
<<<<<<< HEAD
                if !is_my_shard {
=======
                if !is_my_unit {
>>>>>>> a52d5e050a (apollo_l1_events: replace panic with retry in CatchUpper spawned task (#13328))
                    *num_held_units += 1;
                }
                self.maybe_emit(tree_manager)
            }
        }
    }

    fn maybe_emit(&mut self, tree_manager: &PropellerScheduleManager) -> AddUnitAction {
        match self {
            Self::PostConstruction { num_held_units, reconstructed_message } => {
                if tree_manager.should_receive(*num_held_units) {
                    match reconstructed_message.take() {
                        Some(msg) => AddUnitAction::Emit(msg),
                        None => AddUnitAction::NoOp,
                    }
                } else {
                    AddUnitAction::NoOp
                }
            }
            _ => AddUnitAction::NoOp,
        }
    }

    fn transition_to_post(&mut self, message: Vec<u8>, num_held_units: usize) {
        *self = Self::PostConstruction { reconstructed_message: Some(message), num_held_units };
    }
}

/// Message processor that handles validation and state management for a single message.
pub struct MessageProcessor {
    pub committee_id: CommitteeId,
    pub publisher: PeerId,
    pub nonce: u64,
    pub message_root: MessageRoot,
<<<<<<< HEAD
    pub my_shard_index: UnitIndex,
=======
    pub my_unit_index: UnitIndex,
>>>>>>> 0619ce9469 (apollo_propeller: fix engine.rs comments to use 'unit' instead of 'shard' when referring to PropellerUnit)

    pub publisher_public_key: PublicKey,
    pub tree_manager: Arc<PropellerScheduleManager>,
    pub local_peer_id: PeerId,

    // Unbounded because these bridge sync -> async contexts and unit messages from the network
    // must not be dropped or delayed.
    pub unit_rx: mpsc::UnboundedReceiver<UnitToValidate>,
    pub engine_tx: mpsc::UnboundedSender<EventStateManagerToEngine>,

    pub timeout: Duration,
}

impl MessageProcessor {
    pub async fn run(mut self) {
        debug!(
            "[MSG_PROC] Started for committee_id={:?} publisher={:?} root={:?}",
            self.committee_id, self.publisher, self.message_root
        );

        let timed_out = tokio::time::timeout(self.timeout, self.process_units()).await.is_err();

        if timed_out {
            self.emit_timeout_and_finalize();
        }

        debug!(
            "[MSG_PROC] Stopped for committee_id={:?} publisher={:?} root={:?}",
            self.committee_id, self.publisher, self.message_root
        );
    }

    async fn process_units(&mut self) {
        let mut validator = UnitValidator::new(
            self.committee_id,
            self.publisher,
            self.publisher_public_key.clone(),
            self.message_root,
            Arc::clone(&self.tree_manager),
        );
        let mut state = ReconstructionState::new();

        while let Some((sender, unit)) = self.unit_rx.recv().await {
            // TODO(AndrewL): finalize immediately if first validation fails (DOS attack vector)
            trace!("[MSG_PROC] Validating unit from sender={:?} index={:?}", sender, unit.index());

            // TODO(AndrewL): consider processing multiple units simultaneously instead of
            // sequentially.
            let (result, returned_validator, unit) =
                Self::validate_blocking(validator, sender, unit).await;
            validator = returned_validator;

            if let Err(err) = result {
                // TODO(AndrewL): penalize sender of bad unit.
                trace!("[MSG_PROC] Validation failed for index={:?}: {:?}", unit.index(), err);
                continue;
            }

            self.maybe_broadcast_my_unit(&unit, &state);

            let action = state.add_unit(unit, self.my_unit_index, &self.tree_manager);
            if self.handle_action(action, &mut state).await.is_break() {
                return;
            }
        }

        trace!(
            "[MSG_PROC] All senders closed for committee_id={:?} publisher={:?} root={:?}",
            self.committee_id,
            self.publisher,
            self.message_root
        );
        self.finalize();
    }

    /// Offloads CPU-bound validation (signature verification, merkle proofs) to a blocking thread
    /// to avoid blocking the tokio runtime.
    async fn validate_blocking(
        mut validator: UnitValidator,
        sender: PeerId,
        unit: PropellerUnit,
    ) -> ValidationResult {
        // TODO(AndrewL): track task handle to abort the task if the timeout is reached or
        // finalization occurs.
        tokio::task::spawn_blocking(move || {
            let result = validator.validate_unit(sender, &unit);
            (result, validator, unit)
        })
        .await
        .expect("Validation task panicked")
    }

    /// Broadcasts our unit to peers the first time we see it. In PostConstruction this is a no-op
    /// because reconstruction already triggered the broadcast.
    fn maybe_broadcast_my_unit(&self, unit: &PropellerUnit, state: &ReconstructionState) {
<<<<<<< HEAD
        if unit.index() == self.my_shard_index && !state.did_broadcast_my_unit() {
=======
        if unit.index() == self.my_unit_index && !state.did_broadcast_my_unit() {
>>>>>>> 0619ce9469 (apollo_propeller: fix engine.rs comments to use 'unit' instead of 'shard' when referring to PropellerUnit)
            self.broadcast_unit(unit);
        }
    }

    fn broadcast_unit(&self, unit: &PropellerUnit) {
        let mut peers: Vec<PeerId> = self
            .tree_manager
            .get_nodes()
            .iter()
            .map(|(p, _)| *p)
            .filter(|p| *p != self.publisher && *p != self.local_peer_id)
            .collect();
        // TODO(AndrewL): get seeded RNG for tests.
        peers.shuffle(&mut rand::thread_rng());
        trace!("[MSG_PROC] Broadcasting unit index={:?} to {} peers", unit.index(), peers.len());
        self.engine_tx
            .send(EventStateManagerToEngine::SendUnitToPeers { unit: unit.clone(), peers })
            .expect("Engine task has exited");
    }

    async fn handle_action(
        &self,
        action: AddUnitAction,
        state: &mut ReconstructionState,
    ) -> ControlFlow<()> {
        match action {
            AddUnitAction::NoOp => ControlFlow::Continue(()),
            AddUnitAction::Emit(message) => {
                trace!("[MSG_PROC] Emit threshold reached, emitting message");
                self.emit_and_finalize(Event::MessageReceived {
                    publisher: self.publisher,
                    message_root: self.message_root,
                    message,
                })
            }
            AddUnitAction::Reconstruct(units) => {
                let unit_count = units.len();
                trace!("[MSG_PROC] Starting reconstruction with {} units", unit_count);
                match self.reconstruct_blocking(units).await {
                    Ok(output) => self.handle_reconstruction_output(output, unit_count, state),
                    Err(e) => {
                        error!("[MSG_PROC] Reconstruction failed: {:?}", e);
                        self.emit_and_finalize(Event::MessageReconstructionFailed {
                            publisher: self.publisher,
                            message_root: self.message_root,
                            error: e,
                        })
                    }
                }
            }
        }
    }

    /// Offloads erasure-coding reconstruction to a blocking thread.
    async fn reconstruct_blocking(&self, units: Vec<PropellerUnit>) -> ReconstructionResult {
        let message_root = self.message_root;
        let my_index: usize =
            self.my_unit_index.0.try_into().expect("Unit index could not be converted to usize");
        let data_count = self.tree_manager.num_data_shards();
        let coding_count = self.tree_manager.num_coding_shards();

        // TODO(AndrewL): track task handle to abort the task if the timeout is reached or
        // finalization occurs.
        tokio::task::spawn_blocking(move || {
            reconstruct_data_shards(units, message_root, my_index, data_count, coding_count).map(
                |(message, my_shards, my_shard_proof)| ReconstructionOutput {
                    message,
                    my_shards,
                    my_shard_proof,
                },
            )
        })
        .await
        .expect("Reconstruction task panicked")
    }

    fn handle_reconstruction_output(
        &self,
        output: ReconstructionOutput,
        unit_count: usize,
        state: &mut ReconstructionState,
    ) -> ControlFlow<()> {
        let ReconstructionOutput { message, my_shards, my_shard_proof } = output;

        let should_broadcast = !state.did_broadcast_my_unit();
        if should_broadcast {
            let (signature, nonce) = match state {
                ReconstructionState::PreConstruction { verified_fields, .. } => {
                    let parts = verified_fields.as_ref().expect("Verified fields must exist");
                    (parts.signature.clone(), parts.nonce)
                }
                ReconstructionState::PostConstruction { .. } => {
                    unreachable!("Cannot be PostConstruction before transition")
                }
            };
            let reconstructed_unit = PropellerUnit::new(
                self.committee_id,
                self.publisher,
                self.message_root,
                signature,
                self.my_unit_index,
                my_shards,
                my_shard_proof,
                nonce,
            );
            self.broadcast_unit(&reconstructed_unit);
        }

        let total_units = unit_count + usize::from(should_broadcast);
        state.transition_to_post(message, total_units);

        match state.maybe_emit(&self.tree_manager) {
            AddUnitAction::Emit(message) => {
                trace!("[MSG_PROC] Emit threshold reached, emitting message");
                self.emit_and_finalize(Event::MessageReceived {
                    publisher: self.publisher,
                    message_root: self.message_root,
                    message,
                })
            }
            _ => ControlFlow::Continue(()),
        }
    }

    fn emit_timeout_and_finalize(&self) {
        trace!(
            "[MSG_PROC] Timeout reached for committee_id={:?} publisher={:?} root={:?}",
            self.committee_id,
            self.publisher,
            self.message_root
        );
        let _ = self.emit_and_finalize(Event::MessageTimeout {
            committee_id: self.committee_id,
            publisher: self.publisher,
            message_root: self.message_root,
        });
    }

    fn emit_and_finalize(&self, event: Event) -> ControlFlow<()> {
        self.engine_tx
            .send(EventStateManagerToEngine::BehaviourEvent(event))
            .expect("Engine task has exited");
        self.finalize();
        ControlFlow::Break(())
    }

    fn finalize(&self) {
        self.engine_tx
            .send(EventStateManagerToEngine::Finalized {
                committee_id: self.committee_id,
                publisher: self.publisher,
                nonce: self.nonce,
                message_root: self.message_root,
            })
            .expect("Engine task has exited");
    }
}
