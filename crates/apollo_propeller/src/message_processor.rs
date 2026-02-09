//! Message processor combining validation and state management.
//!
//! This module merges the validator and state manager tasks into a single task
//! to eliminate shared fate coordination complexity while maintaining parallelism
//! between validation and reconstruction operations.

use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;

use libp2p::identity::{PeerId, PublicKey};
use rand::seq::SliceRandom;
use tokio::sync::{mpsc, oneshot};
use tokio::time::sleep_until;

use crate::sharding::rebuild_message;
use crate::tree::PropellerScheduleManager;
use crate::types::{Channel, Event, MessageRoot, ReconstructionError, ShardValidationError};
use crate::unit::PropellerUnit;
use crate::unit_validator::UnitValidator;
use crate::{MerkleProof, ShardIndex};

pub type UnitToValidate = (PeerId, PropellerUnit);
type ValidationResult = (Result<(), ShardValidationError>, UnitValidator, PropellerUnit);
type ReconstructionResult = Result<ReconstructionSuccess, ReconstructionError>;

/// Messages sent from MessageProcessor to Engine.
#[derive(Debug)]
pub enum StateManagerToEngine {
    /// An event to be emitted by the behaviour.
    Event(Event),
    /// The message processing has been finalized.
    Finalized { channel: Channel, publisher: PeerId, message_root: MessageRoot },
    /// Broadcast a unit to the specified peers
    BroadcastUnit { unit: PropellerUnit, peers: Vec<PeerId> },
}

/// Successful reconstruction result.specified
#[derive(Debug)]
struct ReconstructionSuccess {
    message: Vec<u8>,
    my_shard: Vec<u8>,
    my_shard_proof: MerkleProof,
}

/// State machine for message reconstruction lifecycle.
enum ReconstructionPhase {
    /// Collecting shards before reconstruction.
    PreReconstruction {
        received_shards: Vec<PropellerUnit>,
        received_my_index: bool,
        signature: Option<Vec<u8>>,
    },
    /// After reconstruction completes, waiting for access threshold.
    PostReconstruction {
        reconstructed_message: Vec<u8>,
        /// Count when reconstruction started (to track additional shards received).
        count_at_reconstruction: usize,
        /// Number of additional shards received after reconstruction.
        additional_shards: usize,
        received_my_index: bool,
    },
}

impl ReconstructionPhase {
    fn was_my_shard_broadcasted(&self) -> bool {
        match self {
            ReconstructionPhase::PreReconstruction { received_my_index, .. } => *received_my_index,
            ReconstructionPhase::PostReconstruction { .. } => true,
        }
    }
}

/// Message processor that handles validation and state management for a single message.
pub struct MessageProcessor {
    // Message identification (needed across methods)
    pub channel: Channel,
    pub publisher: PeerId,
    pub message_root: MessageRoot,
    pub my_shard_index: ShardIndex,

    // Components (needed across methods)
    pub publisher_public_key: PublicKey,
    pub tree_manager: Arc<PropellerScheduleManager>,
    pub local_peer_id: PeerId,

    // Communication channels (needed across methods)
    pub unit_rx: mpsc::UnboundedReceiver<UnitToValidate>,
    pub engine_tx: mpsc::UnboundedSender<StateManagerToEngine>,

    // Timeout
    pub timeout: Duration,
}

impl MessageProcessor {
    pub async fn run(mut self) {
        tracing::trace!(
            "[MSG_PROC] Started for channel={:?} publisher={:?} root={:?}",
            self.channel,
            self.publisher,
            self.message_root
        );

        // Local state variables
        let deadline = tokio::time::Instant::now() + self.timeout;
        let mut validator = Some(UnitValidator::new(
            self.channel,
            self.publisher,
            self.publisher_public_key.clone(),
            self.message_root,
            Arc::clone(&self.tree_manager),
        ));
        let mut pending_validation: Option<oneshot::Receiver<ValidationResult>> = None;
        let mut pending_reconstruction: Option<oneshot::Receiver<ReconstructionResult>> = None;

        // State machine: PreReconstruction -> PostReconstruction
        let mut phase = ReconstructionPhase::PreReconstruction {
            received_shards: Vec::new(),
            received_my_index: false,
            signature: None,
        };

        loop {
            tokio::select! {
                _ = sleep_until(deadline) => {
                    let _ = self.emit_timeout_and_finalize().await;
                    break;
                }

                Some((sender, unit)) = self.unit_rx.recv(), if pending_validation.is_none() => {
                    tracing::trace!("[MSG_PROC] Validating unit from sender={:?} index={:?}", sender, unit.index());

                    let (result_tx, result_rx) = oneshot::channel();
                    let mut validator_moved = validator.take().unwrap();

                    rayon::spawn(move || {
                        let r = validator_moved.validate_shard(sender, &unit);
                        let _ = result_tx.send((r, validator_moved, unit));
                    });

                    pending_validation = Some(result_rx);
                }

                Ok(result) = async {
                    pending_validation.as_mut().unwrap().await
                }, if pending_validation.is_some() => {
                    pending_validation = None;
                    let flow = self.handle_validation_result(
                        result,
                        &mut validator,
                        &mut phase,
                        &mut pending_reconstruction,
                    ).await;
                    if flow.is_break() {
                        break;
                    }
                }

                Ok(result) = async {
                    pending_reconstruction.as_mut().unwrap().await
                }, if pending_reconstruction.is_some() => {
                    pending_reconstruction = None;
                    let flow = self.handle_reconstruction_result(result, &mut phase).await;
                    if flow.is_break() {
                        break;
                    }
                }

                else => {
                    tracing::trace!(
                        "[MSG_PROC] All channels closed for channel={:?} publisher={:?} root={:?}",
                        self.channel,
                        self.publisher,
                        self.message_root
                    );
                    self.engine_tx
                        .send(StateManagerToEngine::Finalized {
                            channel: self.channel,
                            publisher: self.publisher,
                            message_root: self.message_root,
                        })
                        .expect("Engine task has exited");
                    break;
                }
            }
        }

        tracing::trace!(
            "[MSG_PROC] Stopped for channel={:?} publisher={:?} root={:?}",
            self.channel,
            self.publisher,
            self.message_root
        );
    }

    async fn handle_validation_result(
        &mut self,
        result: ValidationResult,
        validator: &mut Option<UnitValidator>,
        phase: &mut ReconstructionPhase,
        pending_reconstruction: &mut Option<oneshot::Receiver<ReconstructionResult>>,
    ) -> ControlFlow<()> {
        // Restore validator
        let (validation_result, validator_returned, unit) = result;
        *validator = Some(validator_returned);

        // Early return for validation errors
        let Err(err) = validation_result else {
            return self.handle_validated_unit(unit, phase, pending_reconstruction).await;
        };

        tracing::trace!(
            "[MSG_PROC] Unit validation failed index={:?} error={:?}",
            unit.index(),
            err
        );
        ControlFlow::Continue(())
    }

    async fn handle_validated_unit(
        &mut self,
        unit: PropellerUnit,
        phase: &mut ReconstructionPhase,
        pending_reconstruction: &mut Option<oneshot::Receiver<ReconstructionResult>>,
    ) -> ControlFlow<()> {
        tracing::trace!("[MSG_PROC] Unit validated successfully index={:?}", unit.index());

        let unit_index = unit.index();

        // Broadcast our shard if we just received it
        if unit_index == self.my_shard_index && !phase.was_my_shard_broadcasted() {
            self.broadcast_shard(&unit).await;
        }

        // Update received_my_index if applicable
        let is_my_shard = unit_index == self.my_shard_index;

        match phase {
            ReconstructionPhase::PreReconstruction {
                received_shards,
                received_my_index,
                signature,
            } => {
                if is_my_shard {
                    *received_my_index = true;
                }
                self.handle_pre_reconstruction_unit(
                    unit,
                    received_shards,
                    signature,
                    pending_reconstruction,
                )
                .await;
                ControlFlow::Continue(())
            }
            ReconstructionPhase::PostReconstruction {
                reconstructed_message,
                count_at_reconstruction,
                additional_shards,
                received_my_index,
            } => {
                if is_my_shard {
                    *received_my_index = true;
                } else {
                    // Increment counter for non-my-shard units received after reconstruction
                    *additional_shards += 1;
                }
                self.check_access_threshold_and_emit(
                    reconstructed_message,
                    *count_at_reconstruction,
                    *additional_shards,
                    *received_my_index,
                )
                .await
            }
        }
    }

    async fn handle_pre_reconstruction_unit(
        &mut self,
        unit: PropellerUnit,
        received_shards: &mut Vec<PropellerUnit>,
        signature: &mut Option<Vec<u8>>,
        pending_reconstruction: &mut Option<oneshot::Receiver<ReconstructionResult>>,
    ) {
        // Store the signature from the first unit we receive
        if signature.is_none() && !unit.signature().is_empty() {
            *signature = Some(unit.signature().to_vec());
        }

        received_shards.push(unit);

        // Check if we should start reconstruction
        if !self.tree_manager.should_build(received_shards.len())
            || pending_reconstruction.is_some()
        {
            return;
        }

        tracing::trace!("[MSG_PROC] Starting reconstruction with {} shards", received_shards.len());

        let shards = received_shards.clone();
        let (tx, rx) = oneshot::channel();
        *pending_reconstruction = Some(rx);
        Self::spawn_reconstruction_task(
            shards,
            self.message_root,
            self.my_shard_index.0.try_into().unwrap(),
            self.tree_manager.num_data_shards(),
            self.tree_manager.num_coding_shards(),
            tx,
        );
    }

    async fn check_access_threshold_and_emit(
        &mut self,
        reconstructed_message: &mut Vec<u8>,
        count_at_reconstruction: usize,
        additional_shards: usize,
        received_my_index: bool,
    ) -> ControlFlow<()> {
        let access_count =
            count_at_reconstruction + additional_shards + usize::from(!received_my_index);

        if !self.tree_manager.should_receive(access_count) {
            return ControlFlow::Continue(());
        }
        tracing::trace!("[MSG_PROC] Access threshold reached, emitting message");
        self.emit_and_finalize(Event::MessageReceived {
            publisher: self.publisher,
            message_root: self.message_root,
            message: std::mem::take(reconstructed_message),
        })
    }

    async fn handle_reconstruction_result(
        &mut self,
        result: ReconstructionResult,
        phase: &mut ReconstructionPhase,
    ) -> ControlFlow<()> {
        tracing::trace!("[MSG_PROC] Reconstruction complete, success={}", result.is_ok());

        // Early return for reconstruction errors
        let Err(e) = result else {
            return self.handle_reconstruction_success(result.unwrap(), phase).await;
        };

        tracing::error!("[MSG_PROC] Reconstruction failed: {:?}", e);

        self.emit_and_finalize(Event::MessageReconstructionFailed {
            publisher: self.publisher,
            message_root: self.message_root,
            error: e,
        })
    }

    async fn handle_reconstruction_success(
        &mut self,
        success: ReconstructionSuccess,
        phase: &mut ReconstructionPhase,
    ) -> ControlFlow<()> {
        let ReconstructionSuccess { message, my_shard, my_shard_proof } = success;

        // Extract pre-reconstruction state and transition to post-reconstruction
        let ReconstructionPhase::PreReconstruction {
            received_shards,
            received_my_index,
            signature,
        } = phase
        else {
            panic!("Expected PreReconstruction phase");
        };

        let count_at_reconstruction = received_shards.len();
        let received_my_index = *received_my_index;
        let signature = signature.take().expect("Signature must exist");

        // Broadcast our shard if we haven't already
        if !phase.was_my_shard_broadcasted() {
            let reconstructed_unit = PropellerUnit::new(
                self.channel,
                self.publisher,
                self.message_root,
                signature,
                self.my_shard_index,
                my_shard,
                my_shard_proof,
            );
            self.broadcast_shard(&reconstructed_unit).await;
        }

        // Check if we can emit immediately
        let access_count = count_at_reconstruction + usize::from(!received_my_index);

        if !self.tree_manager.should_receive(access_count) {
            // Transition to post-reconstruction state
            *phase = ReconstructionPhase::PostReconstruction {
                reconstructed_message: message,
                count_at_reconstruction,
                additional_shards: 0,
                received_my_index,
            };
            return ControlFlow::Continue(());
        }

        tracing::trace!("[MSG_PROC] Access threshold reached immediately after reconstruction");

        self.emit_and_finalize(Event::MessageReceived {
            publisher: self.publisher,
            message_root: self.message_root,
            message,
        })
    }

    async fn emit_timeout_and_finalize(&mut self) -> ControlFlow<()> {
        tracing::trace!(
            "[MSG_PROC] Timeout reached for channel={:?} publisher={:?} root={:?}",
            self.channel,
            self.publisher,
            self.message_root
        );

        self.emit_and_finalize(Event::MessageTimeout {
            channel: self.channel,
            publisher: self.publisher,
            message_root: self.message_root,
        })
    }

    fn emit_and_finalize(&self, event: Event) -> ControlFlow<()> {
        self.engine_tx.send(StateManagerToEngine::Event(event)).expect("Engine task has exited");
        self.engine_tx
            .send(StateManagerToEngine::Finalized {
                channel: self.channel,
                publisher: self.publisher,
                message_root: self.message_root,
            })
            .expect("Engine task has exited");
        ControlFlow::Break(())
    }

    async fn broadcast_shard(&self, unit: &PropellerUnit) {
        let mut peers: Vec<PeerId> = self
            .tree_manager
            .get_nodes()
            .iter()
            .map(|(p, _)| *p)
            .filter(|p| *p != self.publisher && *p != self.local_peer_id)
            .collect();
        peers.shuffle(&mut rand::thread_rng());
        tracing::trace!(
            "[MSG_PROC] Broadcasting unit index={:?} to {} peers",
            unit.index(),
            peers.len()
        );
        self.engine_tx
            .send(StateManagerToEngine::BroadcastUnit { unit: unit.clone(), peers })
            .expect("Engine task has exited");
    }

    fn spawn_reconstruction_task(
        shards: Vec<PropellerUnit>,
        message_root: MessageRoot,
        my_shard_index: usize,
        data_count: usize,
        coding_count: usize,
        result_tx: oneshot::Sender<ReconstructionResult>,
    ) {
        rayon::spawn(move || {
            let result =
                rebuild_message(shards, message_root, my_shard_index, data_count, coding_count)
                    .map(|(message, my_shard, my_shard_proof)| ReconstructionSuccess {
                        message,
                        my_shard,
                        my_shard_proof,
                    });

            let _ = result_tx.send(result);
        });
    }
}
