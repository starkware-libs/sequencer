use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;

use libp2p::identity::{PeerId, PublicKey};
use rand::seq::SliceRandom;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, trace};

use crate::sharding::reconstruct_message_from_shards;
use crate::tree::PropellerScheduleManager;
use crate::types::{Channel, Event, MessageRoot, ReconstructionError, ShardValidationError};
use crate::unit::PropellerUnit;
use crate::unit_validator::UnitValidator;
use crate::{MerkleProof, ShardIndex};

pub type UnitToValidate = (PeerId, PropellerUnit);
type ValidationResult = (Result<(), ShardValidationError>, UnitValidator, PropellerUnit);
type ReconstructionResult = Result<ReconstructionSuccess, ReconstructionError>;

#[derive(Debug)]
pub enum EventStateManagerToEngine {
    BehaviourEvent(Event),
    Finalized { channel: Channel, publisher: PeerId, message_root: MessageRoot },
    BroadcastUnit { unit: PropellerUnit, peers: Vec<PeerId> },
}

#[derive(Debug)]
struct ReconstructionSuccess {
    message: Vec<u8>,
    my_shard: Vec<u8>,
    my_shard_proof: MerkleProof,
}

/// Tracks reconstruction progress for a single message.
struct ReconstructionState {
    received_shards: Vec<PropellerUnit>,
    received_my_index: bool,
    signature: Option<Vec<u8>>,
    reconstructed_message: Option<Vec<u8>>,
    count_at_reconstruction: usize,
    additional_shards_after_reconstruction: usize,
}

impl ReconstructionState {
    fn new() -> Self {
        Self {
            received_shards: Vec::new(),
            received_my_index: false,
            signature: None,
            reconstructed_message: None,
            count_at_reconstruction: 0,
            additional_shards_after_reconstruction: 0,
        }
    }

    fn is_reconstructed(&self) -> bool {
        self.reconstructed_message.is_some()
    }

    fn record_shard(&mut self, is_my_shard: bool) {
        if is_my_shard {
            self.received_my_index = true;
        } else if self.is_reconstructed() {
            self.additional_shards_after_reconstruction += 1;
        }
    }

    fn capture_signature(&mut self, unit: &PropellerUnit) {
        if self.signature.is_none() {
            self.signature = Some(unit.signature().to_vec());
        }
    }

    /// Total shard count used for the access-threshold check.
    fn access_count(&self) -> usize {
        self.count_at_reconstruction
            + self.additional_shards_after_reconstruction
            + usize::from(!self.received_my_index)
    }
}

/// Message processor that handles validation and state management for a single message.
pub struct MessageProcessor {
    pub channel: Channel,
    pub publisher: PeerId,
    pub message_root: MessageRoot,
    pub my_shard_index: ShardIndex,

    pub publisher_public_key: PublicKey,
    pub tree_manager: Arc<PropellerScheduleManager>,
    pub local_peer_id: PeerId,

    // Unbounded because these bridge sync -> async contexts and shard messages from the network
    // must not be dropped or delayed.
    pub unit_rx: mpsc::UnboundedReceiver<UnitToValidate>,
    pub engine_tx: mpsc::UnboundedSender<EventStateManagerToEngine>,

    pub timeout: Duration,
}

impl MessageProcessor {
    pub async fn run(mut self) {
        debug!(
            "[MSG_PROC] Started for channel={:?} publisher={:?} root={:?}",
            self.channel, self.publisher, self.message_root
        );

        let timed_out = tokio::time::timeout(self.timeout, self.process_units()).await.is_err();

        if timed_out {
            self.emit_timeout_and_finalize();
        }

        debug!(
            "[MSG_PROC] Stopped for channel={:?} publisher={:?} root={:?}",
            self.channel, self.publisher, self.message_root
        );
    }

    async fn process_units(&mut self) {
        let mut validator = UnitValidator::new(
            self.channel,
            self.publisher,
            self.publisher_public_key.clone(),
            self.message_root,
            Arc::clone(&self.tree_manager),
        );
        let mut state = ReconstructionState::new();

        while let Some((sender, unit)) = self.unit_rx.recv().await {
            // TODO(AndrewL): finalize immediately if first validation fails (DOS attack vector)
            trace!("[MSG_PROC] Validating unit from sender={:?} index={:?}", sender, unit.index());

            let (result, returned_validator, unit) =
                Self::validate_on_rayon(validator, sender, unit).await;
            validator = returned_validator;

            if let Err(err) = result {
                trace!("[MSG_PROC] Validation failed for index={:?}: {:?}", unit.index(), err);
                continue;
            }

            self.maybe_broadcast_my_shard(&unit, &state);
            state.record_shard(unit.index() == self.my_shard_index);
            state.capture_signature(&unit);

            if self.advance_reconstruction(unit, &mut state).await.is_break() {
                return;
            }
        }

        trace!(
            "[MSG_PROC] All channels closed for channel={:?} publisher={:?} root={:?}",
            self.channel,
            self.publisher,
            self.message_root
        );
        self.finalize();
    }

    // --- Validation --------------------------------------------------------

    /// Offloads CPU-bound validation (signature verification, merkle proofs) to the rayon thread
    /// pool to avoid blocking the tokio runtime. Benchmarked to outperform `spawn_blocking`.
    async fn validate_on_rayon(
        mut validator: UnitValidator,
        sender: PeerId,
        unit: PropellerUnit,
    ) -> ValidationResult {
        let (tx, rx) = oneshot::channel();
        rayon::spawn(move || {
            let result = validator.validate_shard(sender, &unit);
            let _ = tx.send((result, validator, unit));
        });
        rx.await.expect("Validation task panicked")
    }

    // --- Broadcasting ------------------------------------------------------

    fn maybe_broadcast_my_shard(&self, unit: &PropellerUnit, state: &ReconstructionState) {
        if unit.index() == self.my_shard_index && !state.received_my_index {
            self.broadcast_shard(unit);
        }
    }

    fn broadcast_shard(&self, unit: &PropellerUnit) {
        let mut peers: Vec<PeerId> = self
            .tree_manager
            .get_nodes()
            .iter()
            .map(|(p, _)| *p)
            .filter(|p| *p != self.publisher && *p != self.local_peer_id)
            .collect();
        peers.shuffle(&mut rand::thread_rng());
        trace!("[MSG_PROC] Broadcasting unit index={:?} to {} peers", unit.index(), peers.len());
        self.engine_tx
            .send(EventStateManagerToEngine::BroadcastUnit { unit: unit.clone(), peers })
            .expect("Engine task has exited");
    }

    // --- Reconstruction ----------------------------------------------------

    async fn advance_reconstruction(
        &self,
        unit: PropellerUnit,
        state: &mut ReconstructionState,
    ) -> ControlFlow<()> {
        if state.is_reconstructed() {
            return self.maybe_emit_message(state);
        }

        state.received_shards.push(unit);

        if !self.tree_manager.should_build(state.received_shards.len()) {
            return ControlFlow::Continue(());
        }

        trace!("[MSG_PROC] Starting reconstruction with {} shards", state.received_shards.len());
        state.count_at_reconstruction = state.received_shards.len();

        match self.reconstruct_on_rayon(state).await {
            Ok(success) => self.handle_reconstruction_success(success, state),
            Err(e) => {
                tracing::error!("[MSG_PROC] Reconstruction failed: {:?}", e);
                self.emit_and_finalize(Event::MessageReconstructionFailed {
                    publisher: self.publisher,
                    message_root: self.message_root,
                    error: e,
                })
            }
        }
    }

    /// Offloads erasure-coding reconstruction to rayon.
    async fn reconstruct_on_rayon(&self, state: &mut ReconstructionState) -> ReconstructionResult {
        let shards = std::mem::take(&mut state.received_shards);
        let message_root = self.message_root;
        let my_index: usize = self.my_shard_index.0.try_into().unwrap();
        let data_count = self.tree_manager.num_data_shards();
        let coding_count = self.tree_manager.num_coding_shards();

        let (tx, rx) = oneshot::channel();
        rayon::spawn(move || {
            let result = reconstruct_message_from_shards(
                shards,
                message_root,
                my_index,
                data_count,
                coding_count,
            )
            .map(|(message, my_shard, my_shard_proof)| ReconstructionSuccess {
                message,
                my_shard,
                my_shard_proof,
            });
            let _ = tx.send(result);
        });
        rx.await.expect("Reconstruction task panicked")
    }

    fn handle_reconstruction_success(
        &self,
        success: ReconstructionSuccess,
        state: &mut ReconstructionState,
    ) -> ControlFlow<()> {
        let ReconstructionSuccess { message, my_shard, my_shard_proof } = success;

        if !state.received_my_index {
            let signature = state.signature.clone().expect("Signature must exist");
            let reconstructed_unit = PropellerUnit::new(
                self.channel,
                self.publisher,
                self.message_root,
                signature,
                self.my_shard_index,
                my_shard,
                my_shard_proof,
            );
            self.broadcast_shard(&reconstructed_unit);
        }

        state.reconstructed_message = Some(message);
        self.maybe_emit_message(state)
    }

    // --- Emission / finalization -------------------------------------------

    fn maybe_emit_message(&self, state: &mut ReconstructionState) -> ControlFlow<()> {
        if !self.tree_manager.should_receive(state.access_count()) {
            return ControlFlow::Continue(());
        }

        trace!("[MSG_PROC] Access threshold reached, emitting message");
        let message = state.reconstructed_message.take().expect("Message must exist");
        self.emit_and_finalize(Event::MessageReceived {
            publisher: self.publisher,
            message_root: self.message_root,
            message,
        })
    }

    fn emit_timeout_and_finalize(&self) {
        trace!(
            "[MSG_PROC] Timeout reached for channel={:?} publisher={:?} root={:?}",
            self.channel,
            self.publisher,
            self.message_root
        );
        let _ = self.emit_and_finalize(Event::MessageTimeout {
            channel: self.channel,
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
                channel: self.channel,
                publisher: self.publisher,
                message_root: self.message_root,
            })
            .expect("Engine task has exited");
    }
}
