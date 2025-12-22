//! Validator task for per-message shard validation.

use std::time::Duration;

use libp2p::identity::PeerId;
use tokio::sync::{mpsc, oneshot};
use tokio::time::sleep_until;

use super::task_messages::ValidatorToStateManager;
use crate::channel_utils::try_send_or_exit;
use crate::deadline_wrapper::spawn_monitored;
use crate::types::{Channel, MessageRoot};
use crate::unit::PropellerUnit;
use crate::unit_validator::UnitValidator;

/// A unit to be validated, sent from Core to Validator task.
#[derive(Debug)]
pub(crate) struct UnitToValidate {
    pub sender: PeerId,
    pub unit: PropellerUnit,
}

/// Handle for communicating with a validator task.
pub(crate) struct ValidatorTaskHandle {
    /// Sender for units to validate.
    pub unit_tx: mpsc::Sender<UnitToValidate>,
    /// Sender for state manager to validator messages.
    pub sm_to_validator_tx: mpsc::Sender<super::task_messages::StateManagerToValidator>,
}

/// Spawns a validator task for a specific message.
///
/// The validator task:
/// - Receives units to validate from Core via bounded channel
/// - Validates each unit using the UnitValidator
/// - Sends validated units to the State Manager
/// - Times out after the configured duration
/// - On timeout, sends ValidatorStopped to State Manager
pub(crate) fn spawn_validator_task(
    channel: Channel,
    publisher: PeerId,
    message_root: MessageRoot,
    validator: UnitValidator,
    timeout: Duration,
    channel_capacity: usize,
    state_manager_tx: mpsc::Sender<ValidatorToStateManager>,
) -> ValidatorTaskHandle {
    let (unit_tx, unit_rx) = mpsc::channel(channel_capacity);
    let (sm_to_validator_tx, sm_to_validator_rx) = mpsc::channel(channel_capacity);

    spawn_monitored("validator_task", async move {
        run_validator_task(
            channel,
            publisher,
            message_root,
            validator,
            timeout,
            unit_rx,
            sm_to_validator_rx,
            state_manager_tx,
        )
        .await;
    });

    ValidatorTaskHandle { unit_tx, sm_to_validator_tx }
}

async fn run_validator_task(
    channel: Channel,
    publisher: PeerId,
    message_root: MessageRoot,
    validator: UnitValidator,
    timeout: Duration,
    mut unit_rx: mpsc::Receiver<UnitToValidate>,
    mut sm_to_validator_rx: mpsc::Receiver<super::task_messages::StateManagerToValidator>,
    state_manager_tx: mpsc::Sender<ValidatorToStateManager>,
) {
    let deadline = tokio::time::Instant::now() + timeout;

    tracing::trace!(
        "[VALIDATOR] Started for channel={} publisher={:?} root={:?} timeout={:?}",
        channel,
        publisher,
        message_root,
        timeout
    );

    let mut validator = Some(validator);

    loop {
        tokio::select! {
            // Receive shutdown from state manager
            Some(msg) = sm_to_validator_rx.recv() => {
                match msg {
                    super::task_messages::StateManagerToValidator::Shutdown => {
                        tracing::trace!(
                            "[VALIDATOR] Received shutdown from state manager for channel={} publisher={:?} root={:?}",
                            channel,
                            publisher,
                            message_root
                        );
                        break;
                    }
                }
            }

            // Check for timeout
            _ = sleep_until(deadline) => {
                tracing::trace!(
                    "[VALIDATOR] Timeout reached for channel={} publisher={:?} root={:?}",
                    channel,
                    publisher,
                    message_root
                );

                // Notify state manager that validator has stopped
                if !try_send_or_exit(
                    &state_manager_tx,
                    ValidatorToStateManager::ValidatorStopped,
                    "Validator->StateManager"
                ) {
                    // Channel closed, exit
                    break;
                }
                break;
            }

            // Receive unit to validate
            Some(unit_to_validate) = unit_rx.recv() => {
                let UnitToValidate { sender, unit } = unit_to_validate;
                let index = unit.index();

                tracing::trace!(
                    "[VALIDATOR] Validating unit from sender={:?} index={:?}",
                    sender,
                    index
                );

                let mut validator_now = validator.take().unwrap();

                let (result_tx, result_rx) = oneshot::channel();
                rayon::spawn(move || {
                    let r = validator_now.validate_shard(sender, &unit);
                    let _ = result_tx.send((r, validator_now, unit));
                });

                let (result, validator_after, unit_after) =
                    result_rx.await.expect("Rayon task failed to send result");

                validator = Some(validator_after);

                match result {
                    Ok(()) => {
                        tracing::trace!(
                            "[VALIDATOR] Unit validated successfully from sender={:?} index={:?}",
                            sender,
                            index
                        );

                        // Send validated unit to state manager
                        let msg = ValidatorToStateManager::ValidatedUnit { sender, unit: unit_after };
                        if !try_send_or_exit(&state_manager_tx, msg, "Validator->StateManager") {
                            // Channel closed, exit gracefully
                            break;
                        }

                        // Yield after sending to prevent exceeding poll deadline
                        tokio::task::yield_now().await;
                    }
                    Err(err) => {
                        tracing::trace!(
                            "[VALIDATOR] Unit validation failed from sender={:?} index={:?} error={:?}",
                            sender,
                            index,
                            err
                        );

                        // State manager will handle validation errors via events
                        // For now, we don't send anything for validation failures
                        // The state machine in state.rs already handles this appropriately
                    }
                }
            }

            // Channel closed - Core has dropped the sender
            else => {
                tracing::trace!(
                    "[VALIDATOR] Unit channel closed for channel={} publisher={:?} root={:?}",
                    channel,
                    publisher,
                    message_root
                );
                break;
            }
        }
    }

    tracing::trace!(
        "[VALIDATOR] Stopped for channel={} publisher={:?} root={:?}",
        channel,
        publisher,
        message_root
    );
}
