use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;

use libp2p::identity::{PeerId, PublicKey};
use rand::seq::SliceRandom;
use tokio::sync::mpsc;
use tracing::{debug, error, trace};

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
    SendUnitToPeers { unit: PropellerUnit, peers: Vec<PeerId> },
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
    broadcast_my_shard: bool,
    signature: Option<Vec<u8>>,
    reconstructed_message: Option<Vec<u8>>,
    shard_count_at_reconstruction: usize,
    shards_received_after_reconstruction: usize,
}

impl ReconstructionState {
    fn new() -> Self {
        Self {
            received_shards: Vec::new(),
            broadcast_my_shard: false,
            signature: None,
            reconstructed_message: None,
            shard_count_at_reconstruction: 0,
            shards_received_after_reconstruction: 0,
        }
    }

    fn is_reconstructed(&self) -> bool {
        self.reconstructed_message.is_some()
    }

    fn record_shard(&mut self, is_my_shard: bool) {
        if is_my_shard {
            self.broadcast_my_shard = true;
        }
        if self.is_reconstructed() {
            self.shards_received_after_reconstruction += 1;
        }
    }

    fn capture_signature(&mut self, unit: &PropellerUnit) {
        if self.signature.is_none() {
            self.signature = Some(unit.signature().to_vec());
        }
    }

    /// Total shard count used for the access-threshold check.
    fn effective_shard_count(&self) -> usize {
        self.shard_count_at_reconstruction
            + self.shards_received_after_reconstruction
            + usize::from(!self.broadcast_my_shard)
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
                Self::validate_blocking(validator, sender, unit).await;
            validator = returned_validator;

            if let Err(err) = result {
                // TODO(AndrewL): penalize sender of bad shard.
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

    /// Offloads CPU-bound validation (signature verification, merkle proofs) to a blocking thread
    /// to avoid blocking the tokio runtime.
    async fn validate_blocking(
        mut validator: UnitValidator,
        sender: PeerId,
        unit: PropellerUnit,
    ) -> ValidationResult {
        tokio::task::spawn_blocking(move || {
            let result = validator.validate_shard(sender, &unit);
            (result, validator, unit)
        })
        .await
        .expect("Validation task panicked")
    }

    fn maybe_broadcast_my_shard(&self, unit: &PropellerUnit, state: &ReconstructionState) {
        if unit.index() == self.my_shard_index && !state.broadcast_my_shard {
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
            .send(EventStateManagerToEngine::SendUnitToPeers { unit: unit.clone(), peers })
            .expect("Engine task has exited");
    }

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
        state.shard_count_at_reconstruction = state.received_shards.len();

        match self.reconstruct_blocking(state).await {
            Ok(success) => self.handle_reconstruction_success(success, state),
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

    /// Offloads erasure-coding reconstruction to a blocking thread.
    async fn reconstruct_blocking(&self, state: &mut ReconstructionState) -> ReconstructionResult {
        let shards = std::mem::take(&mut state.received_shards);
        let message_root = self.message_root;
        let my_index: usize = self.my_shard_index.0.try_into().unwrap();
        let data_count = self.tree_manager.num_data_shards();
        let coding_count = self.tree_manager.num_coding_shards();

        tokio::task::spawn_blocking(move || {
            reconstruct_message_from_shards(
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
            })
        })
        .await
        .expect("Reconstruction task panicked")
    }

    fn handle_reconstruction_success(
        &self,
        success: ReconstructionSuccess,
        state: &mut ReconstructionState,
    ) -> ControlFlow<()> {
        let ReconstructionSuccess { message, my_shard, my_shard_proof } = success;

        if !state.broadcast_my_shard {
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
            state.broadcast_my_shard = true;
        }

        state.reconstructed_message = Some(message);
        self.maybe_emit_message(state)
    }

    fn maybe_emit_message(&self, state: &mut ReconstructionState) -> ControlFlow<()> {
        if !self.tree_manager.should_receive(state.effective_shard_count()) {
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
