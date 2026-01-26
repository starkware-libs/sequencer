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
                    let flow = self.handle_validation_result(result, &mut validator).await;
                    if flow.is_break() {
                        break;
                    }
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
    ) -> ControlFlow<()> {
        // Restore validator
        let (validation_result, validator_returned, unit) = result;
        *validator = Some(validator_returned);

        // Early return for validation errors
        let Err(err) = validation_result else {
            return self.handle_validated_unit(unit).await;
        };

        tracing::trace!(
            "[MSG_PROC] Unit validation failed index={:?} error={:?}",
            unit.index(),
            err
        );
        ControlFlow::Continue(())
    }

    async fn handle_validated_unit(&mut self, unit: PropellerUnit) -> ControlFlow<()> {
        tracing::trace!("[MSG_PROC] Unit validated successfully index={:?}", unit.index());

        let unit_index = unit.index();

        // Broadcast our shard if we just received it
        if unit_index == self.my_shard_index {
            self.broadcast_shard(&unit).await;
        }

        // TODO(AndrewL): Process validated units further
        ControlFlow::Continue(())
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
