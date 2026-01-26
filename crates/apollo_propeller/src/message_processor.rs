use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;

use libp2p::identity::{PeerId, PublicKey};
use tokio::sync::{mpsc, oneshot};
use tokio::time::sleep_until;
use tracing::{debug, trace};

use crate::tree::PropellerScheduleManager;
use crate::types::{Channel, Event, MessageRoot, ShardValidationError};
use crate::unit::PropellerUnit;
use crate::unit_validator::UnitValidator;
use crate::ShardIndex;

pub type UnitToValidate = (PeerId, PropellerUnit);
type ValidationResult = (Result<(), ShardValidationError>, UnitValidator, PropellerUnit);

#[derive(Debug)]
pub enum EventStateManagerToEngine {
    BehaviourEvent(Event),
    Finalized { channel: Channel, publisher: PeerId, message_root: MessageRoot },
    BroadcastUnit { unit: PropellerUnit, peers: Vec<PeerId> },
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
                    // Safe: the guard `pending_validation.is_none()` ensures this arm only
                    // fires when no validation is in flight, and the validator is only taken
                    // while a validation is pending.
                    let mut validator_moved = validator.take().expect(
                        "validator must be present when no validation is pending"
                    );

                    // Validation is CPU-bound (signature verification, merkle proofs).
                    // `rayon::spawn` runs it on a dedicated thread pool to avoid blocking the
                    // tokio async runtime. `tokio::spawn_blocking` was benchmarked and found
                    // to be noticeably worse than rayon for this workload.
                    // The task starts executing immediately in the background; the oneshot
                    // receiver (`pending_validation`) serves as the handle to collect the
                    // result in a future select arm.
                    rayon::spawn(move || {
                        let r = validator_moved.validate_shard(sender, &unit);
                        let _ = result_tx.send((r, validator_moved, unit));
                    });

                    pending_validation = Some(result_rx);
                }

                // TODO(AndrewL): Handle `Err(RecvError)` from the oneshot. If the
                // rayon task panics, the sender is dropped and this arm silently
                // skips the `Err` variant, leaving `pending_validation` stale and
                // the validator permanently lost.
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

        debug!(
            "[MSG_PROC] Stopped for channel={:?} publisher={:?} root={:?}",
            self.channel, self.publisher, self.message_root
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
            tracing::trace!("[MSG_PROC] Unit validated successfully index={:?}", unit.index());
            // TODO(AndrewL): Process validated units
            return ControlFlow::Continue(());
        };

        tracing::trace!(
            "[MSG_PROC] Unit validation failed index={:?} error={:?}",
            unit.index(),
            err
        );
        ControlFlow::Continue(())
    }

    async fn emit_timeout_and_finalize(&mut self) -> ControlFlow<()> {
        trace!(
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
        self.engine_tx
            .send(EventStateManagerToEngine::BehaviourEvent(event))
            .expect("Engine task has exited");
        self.engine_tx
            .send(EventStateManagerToEngine::Finalized {
                channel: self.channel,
                publisher: self.publisher,
                message_root: self.message_root,
            })
            .expect("Engine task has exited");
        ControlFlow::Break(())
    }
}
