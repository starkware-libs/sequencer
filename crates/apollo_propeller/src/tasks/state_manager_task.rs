//! State manager task for per-message state transitions and reconstruction.
//!
//! This task manages the lifecycle of a message:
//! 1. Collect validated shards
//! 2. Broadcast our shard if we receive it
//! 3. Start reconstruction when threshold reached
//! 4. Broadcast reconstructed shard if needed
//! 5. Wait for access threshold
//! 6. Emit message

use std::sync::Arc;

use libp2p::identity::PeerId;
use rand::seq::SliceRandom;
use tokio::sync::{mpsc, oneshot};

use super::task_messages::{StateManagerToCore, StateManagerToValidator, ValidatorToStateManager};
use crate::channel_utils::{send_critical, send_non_critical, ChannelName};
use crate::config::Config;
use crate::deadline_wrapper::spawn_monitored;
use crate::tree::PropellerTreeManager;
use crate::types::{Channel, Event, MessageRoot, ReconstructionError};
use crate::unit::PropellerUnit;
use crate::{MerkleProof, MerkleTree, ShardIndex};

/// Result from a reconstruction task.
#[derive(Debug)]
struct ReconstructionResult {
    result: Result<ReconstructionSuccess, ReconstructionError>,
}

/// Successful reconstruction result.
#[derive(Debug)]
struct ReconstructionSuccess {
    message: Vec<u8>,
    my_shard: Vec<u8>,
    my_shard_proof: MerkleProof,
}

/// Spawns a state manager task for a specific message.
pub(crate) fn spawn_state_manager_task(
    channel: Channel,
    publisher: PeerId,
    message_root: MessageRoot,
    my_shard_index: ShardIndex,
    tree_manager: Arc<PropellerTreeManager>,
    local_peer_id: PeerId,
    config: Config,
    validator_rx: mpsc::Receiver<ValidatorToStateManager>,
    validator_tx: mpsc::Sender<StateManagerToValidator>,
    core_tx: mpsc::Sender<StateManagerToCore>,
) {
    spawn_monitored("state_manager_task", async move {
        run_state_manager_task(
            channel,
            publisher,
            message_root,
            my_shard_index,
            tree_manager,
            local_peer_id,
            config,
            validator_rx,
            validator_tx,
            core_tx,
        )
        .await;
    });
}

pub(crate) async fn run_state_manager_task(
    channel: Channel,
    publisher: PeerId,
    message_root: MessageRoot,
    my_shard_index: ShardIndex,
    tree_manager: Arc<PropellerTreeManager>,
    local_peer_id: PeerId,
    config: Config,
    mut validator_rx: mpsc::Receiver<ValidatorToStateManager>,
    validator_tx: mpsc::Sender<StateManagerToValidator>,
    core_tx: mpsc::Sender<StateManagerToCore>,
) {
    tracing::trace!(
        "[STATE_MGR] Started for channel={} publisher={:?} root={:?}",
        channel,
        publisher,
        message_root
    );

    // Simple state variables
    let mut received_shards = Vec::new();
    let mut received_my_index = false;
    let mut my_shard_broadcasted = false;
    let mut received_count = 0;
    let mut reconstructed_message: Option<Vec<u8>> = None;
    let mut reconstruction_rx: Option<oneshot::Receiver<ReconstructionResult>> = None;
    let mut signature: Option<Vec<u8>> = None;

    loop {
        tokio::select! {
            // Receive message from validator
            Some(msg) = validator_rx.recv() => {
                match msg {
                    ValidatorToStateManager::ValidatedUnit { sender, unit } => {
                        tracing::trace!(
                            "[STATE_MGR] Received validated unit from sender={:?} index={:?}",
                            sender,
                            unit.index()
                        );

                        let unit_index = unit.index();
                        received_count += 1;

                        // Track if we received our own shard
                        if unit_index == my_shard_index {
                            received_my_index = true;
                        }

                        // Broadcast our shard if we just received it
                        if unit_index == my_shard_index && !my_shard_broadcasted {
                            my_shard_broadcasted = true;
                            broadcast_shard(&core_tx, &unit, &tree_manager, publisher, local_peer_id).await;
                        }

                        // Before reconstruction: collect shards
                        if reconstructed_message.is_none() {
                            // Store the signature from the first unit we receive
                            // (all units from the same message have the same signature)
                            if signature.is_none() && !unit.signature().is_empty() {
                                signature = Some(unit.signature().to_vec());
                            }

                            received_shards.push(unit);

                            // Calculate total shards for threshold check
                            let total_shards = received_count;

                            // Check if we should start reconstruction
                            if tree_manager.should_build(total_shards) && reconstruction_rx.is_none() {
                                tracing::trace!(
                                    "[STATE_MGR] Starting reconstruction with {} shards",
                                    received_shards.len()
                                );

                                let shards = received_shards.clone();
                                let (tx, rx) = oneshot::channel();
                                reconstruction_rx = Some(rx);
                                spawn_reconstruction_task(
                                    shards,
                                    message_root,
                                    my_shard_index.0.try_into().unwrap(),
                                    tree_manager.calculate_data_shards(),
                                    tree_manager.calculate_coding_shards(),
                                    config.pad,
                                    tx,
                                );
                            }
                        } else {
                            // After reconstruction: check access threshold
                            let access_count = if received_my_index {
                                received_count
                            } else {
                                received_count + 1
                            };

                            if tree_manager.should_receive(access_count) {
                                tracing::trace!(
                                    "[STATE_MGR] Access threshold reached, emitting message"
                                );

                                let message = reconstructed_message.take().unwrap();
                                let event = Event::MessageReceived { publisher, message_root, message };
                                send_critical(&core_tx, StateManagerToCore::Event(event), ChannelName::StateManagerToCore).await;

                                send_finalized(&core_tx, &validator_tx, channel, publisher, message_root).await;
                                break;
                            }
                        }
                    }
                    ValidatorToStateManager::ValidatorStopped => {
                        tracing::trace!(
                            "[STATE_MGR] Validator stopped (timeout) for channel={} publisher={:?} root={:?}",
                            channel,
                            publisher,
                            message_root
                        );

                        let event = Event::MessageTimeout { channel, publisher, message_root };
                        send_critical(&core_tx, StateManagerToCore::Event(event), ChannelName::StateManagerToCore).await;

                        send_finalized(&core_tx, &validator_tx, channel, publisher, message_root).await;
                        break;
                    }
                }
            }

            // Receive reconstruction result
            Ok(result) = async { reconstruction_rx.as_mut().unwrap().await }, if reconstruction_rx.is_some() => {
                reconstruction_rx = None;
                tracing::trace!(
                    "[STATE_MGR] Reconstruction complete, success={}",
                    result.result.is_ok()
                );

                match result.result {
                    Err(e) => {
                        tracing::error!("[STATE_MGR] Reconstruction failed: {:?}", e);

                        let event = Event::MessageReconstructionFailed { publisher, message_root, error: e };
                        send_critical(&core_tx, StateManagerToCore::Event(event), ChannelName::StateManagerToCore).await;

                        send_finalized(&core_tx, &validator_tx, channel, publisher, message_root).await;
                        break;
                    }
                    Ok(success) => {
                        let ReconstructionSuccess { message, my_shard, my_shard_proof } = success;

                        // Broadcast our shard if we haven't already
                        if !my_shard_broadcasted {
                            my_shard_broadcasted = true;
                            let reconstructed_unit = PropellerUnit::new(
                                channel,
                                publisher,
                                message_root,
                                signature.take().unwrap(),
                                my_shard_index,
                                my_shard,
                                my_shard_proof,
                            );
                            broadcast_shard(&core_tx, &reconstructed_unit, &tree_manager, publisher, local_peer_id).await;
                        }

                        // Store reconstructed message
                        reconstructed_message = Some(message);

                        // Check if we can emit immediately
                        let access_count = if received_my_index {
                            received_count
                        } else {
                            received_count + 1
                        };

                        if tree_manager.should_receive(access_count) {
                            tracing::trace!(
                                "[STATE_MGR] Access threshold reached immediately after reconstruction"
                            );

                            let message = reconstructed_message.take().unwrap();
                            let event = Event::MessageReceived { publisher, message_root, message };
                            send_critical(&core_tx, StateManagerToCore::Event(event), ChannelName::StateManagerToCore).await;

                            send_finalized(&core_tx, &validator_tx, channel, publisher, message_root).await;
                            break;
                        }
                    }
                }
            }

            // All channels closed
            else => {
                tracing::trace!(
                    "[STATE_MGR] All channels closed for channel={} publisher={:?} root={:?}",
                    channel,
                    publisher,
                    message_root
                );
                send_finalized(&core_tx, &validator_tx, channel, publisher, message_root).await;
                break;
            }
        }
    }

    tracing::trace!(
        "[STATE_MGR] Stopped for channel={} publisher={:?} root={:?}",
        channel,
        publisher,
        message_root
    );
}

async fn send_finalized(
    core_tx: &mpsc::Sender<StateManagerToCore>,
    validator_tx: &mpsc::Sender<StateManagerToValidator>,
    channel: Channel,
    publisher: PeerId,
    message_root: MessageRoot,
) {
    let msg = StateManagerToCore::Finalized { channel, publisher, message_root };
    send_critical(core_tx, msg, ChannelName::StateManagerToCore).await;
    let _ = send_non_critical(
        validator_tx,
        StateManagerToValidator::Shutdown,
        ChannelName::StateManagerToValidator,
    )
    .await;
}

async fn broadcast_shard(
    core_tx: &mpsc::Sender<StateManagerToCore>,
    unit: &PropellerUnit,
    tree_manager: &PropellerTreeManager,
    publisher: PeerId,
    local_peer_id: PeerId,
) {
    let mut peers: Vec<PeerId> = tree_manager
        .get_nodes()
        .iter()
        .map(|(p, _)| *p)
        .filter(|p| *p != publisher && *p != local_peer_id)
        .collect();

    peers.shuffle(&mut rand::thread_rng());

    tracing::trace!(
        "[STATE_MGR] Broadcasting unit index={:?} to {} peers",
        unit.index(),
        peers.len()
    );

    let msg = StateManagerToCore::BroadcastUnit { unit: unit.clone(), peers };
    send_critical(core_tx, msg, ChannelName::StateManagerToCore).await;
}

fn spawn_reconstruction_task(
    shards: Vec<PropellerUnit>,
    message_root: MessageRoot,
    my_shard_index: usize,
    data_count: usize,
    coding_count: usize,
    pad: bool,
    result_tx: oneshot::Sender<ReconstructionResult>,
) {
    rayon::spawn(move || {
        let result =
            re_build_message(shards, message_root, my_shard_index, data_count, coding_count)
                .and_then(|(message, my_shard, my_shard_proof)| {
                    let un_padded_message = if pad { un_pad_message(message)? } else { message };
                    Ok(ReconstructionSuccess {
                        message: un_padded_message,
                        my_shard,
                        my_shard_proof,
                    })
                });

        let reconstruction_result = ReconstructionResult { result };
        let _ = result_tx.send(reconstruction_result);
    });
}

fn re_build_message(
    received_shards: Vec<PropellerUnit>,
    message_root: MessageRoot,
    my_shard_index: usize,
    data_count: usize,
    coding_count: usize,
) -> Result<(Vec<u8>, Vec<u8>, MerkleProof), ReconstructionError> {
    let shards_for_reconstruction: Vec<(usize, Vec<u8>)> = received_shards
        .into_iter()
        .map(|mut msg| (msg.index().0.try_into().unwrap(), std::mem::take(msg.shard_mut())))
        .collect();

    let reconstructed_data_shards = crate::reed_solomon::reconstruct_message_from_shards(
        &shards_for_reconstruction,
        data_count,
        coding_count,
    )
    .map_err(ReconstructionError::ErasureReconstructionFailed)?;
    let recreated_coding_shards =
        crate::reed_solomon::generate_coding_shards(&reconstructed_data_shards, coding_count)
            .map_err(ReconstructionError::ErasureReconstructionFailed)?;

    let mut all_shards = [reconstructed_data_shards.clone(), recreated_coding_shards].concat();

    let are_all_shards_the_same_length =
        all_shards.iter().all(|shard| shard.len() == all_shards[0].len());
    if !are_all_shards_the_same_length {
        return Err(ReconstructionError::UnequalShardLengths);
    }

    let merkle_tree = MerkleTree::new(&all_shards);
    let computed_root = MessageRoot(merkle_tree.root());

    if computed_root != message_root {
        return Err(ReconstructionError::MismatchedMessageRoot);
    }

    let message = crate::reed_solomon::combine_data_shards(reconstructed_data_shards);
    Ok((
        message,
        std::mem::take(&mut all_shards[my_shard_index]),
        merkle_tree.prove(my_shard_index).unwrap(),
    ))
}

fn un_pad_message(message: Vec<u8>) -> Result<Vec<u8>, ReconstructionError> {
    if message.len() < 4 {
        return Err(ReconstructionError::MessagePaddingError);
    }

    let length_bytes: [u8; 4] = message[..4].try_into().expect("This should never fail");
    let original_message_length: u32 = u32::from_le_bytes(length_bytes);
    let original_message_length_usize: usize = original_message_length.try_into().unwrap();

    if 4 + original_message_length_usize > message.len() {
        return Err(ReconstructionError::MessagePaddingError);
    }

    Ok(message[4..(4 + original_message_length_usize)].to_vec())
}
