//! Tests for state_manager_task module.
//!
//! This test file documents a TODO bug in state_manager_task.rs at line 217.

use std::sync::Arc;

use libp2p::identity::{Keypair, PeerId};
use tokio::sync::mpsc;

use super::state_manager_task::run_state_manager_task;
use super::task_messages::{StateManagerToCore, ValidatorToStateManager};
use crate::config::Config;
use crate::core::Core;
use crate::tree::PropellerTreeManager;
use crate::types::{Channel, ShardIndex};

#[tokio::test]
async fn test_reconstructed_unit_signature_matches_original() {
    // 1. Create 4 random peer IDs (sorted)
    let mut peer_keypairs: Vec<(PeerId, Keypair)> = (0..4)
        .map(|_| {
            let keypair = Keypair::generate_ed25519();
            let peer_id = PeerId::from(keypair.public());
            (peer_id, keypair)
        })
        .collect();
    peer_keypairs.sort_by_key(|(peer_id, _)| *peer_id);
    let peer_ids: Vec<PeerId> = peer_keypairs.iter().map(|(id, _)| *id).collect();

    // 2. Use first peer as the broadcaster
    let (publisher, publisher_keypair) = peer_keypairs[0].clone();
    let (local_peer_id, _local_keypair) = peer_keypairs[2].clone(); // We are peer 2

    // 3. Generate a message and create all shards
    let channel = Channel(42);
    // Message length must be a multiple of num_data_shards
    let num_data_shards = 1;
    let num_coding_shards = 2;
    // Create a message that's exactly 100 bytes (multiple of 2)
    let message = vec![42u8; 100];

    let all_units = Core::prepare_units(
        channel,
        publisher,
        Some(publisher_keypair),
        message.clone(),
        false, // no padding
        num_data_shards,
        num_coding_shards,
    )
    .expect("Failed to prepare units");

    assert_eq!(all_units.len(), 3, "Should have 3 shards total");

    // Extract the signature from the original units (they all have the same signature)
    let expected_signature = all_units[0].signature().to_vec();
    assert!(!expected_signature.is_empty(), "Original units should have signatures");

    // 4. Create tree manager with all peers
    let peer_weights: Vec<(PeerId, u64)> = peer_ids.iter().map(|id| (*id, 1)).collect();
    let mut tree_manager = PropellerTreeManager::new(local_peer_id);
    tree_manager.update_nodes(peer_weights).expect("Failed to update nodes");
    let tree_manager = Arc::new(tree_manager);

    // Determine which shard index we (local_peer_id) are responsible for
    let my_shard_index =
        tree_manager.get_my_shard_index(&publisher).expect("Should have a shard index");

    // Get the message root from the units
    let message_root = all_units[0].root();

    // 5. Set up channels for state_manager_task
    let (validator_tx, validator_rx) = mpsc::channel(100);
    let (state_mgr_to_validator_tx, mut _state_mgr_to_validator_rx) = mpsc::channel(100);
    let (core_tx, mut core_rx) = mpsc::channel(100);

    let config = Config::builder().pad(false).build();

    // Spawn the state manager task
    let task_handle = tokio::spawn(run_state_manager_task(
        channel,
        publisher,
        message_root,
        my_shard_index,
        tree_manager.clone(),
        local_peer_id,
        config,
        validator_rx,
        state_mgr_to_validator_tx,
        core_tx,
    ));

    // 6. Send shards to the state manager (all except the one we're responsible for)
    for (i, unit) in all_units.iter().enumerate() {
        if ShardIndex(i as u32) != my_shard_index {
            validator_tx
                .send(ValidatorToStateManager::ValidatedUnit {
                    sender: peer_ids[i],
                    unit: unit.clone(),
                })
                .await
                .expect("Failed to send unit");
        }
    }

    // Wait for the state manager to broadcast the reconstructed unit (with timeout)
    let reconstructed_unit;
    let timeout_duration = std::time::Duration::from_secs(5);
    let timeout = tokio::time::sleep(timeout_duration);
    tokio::pin!(timeout);

    reconstructed_unit = loop {
        tokio::select! {
            Some(msg) = core_rx.recv() => {
                match msg {
                    StateManagerToCore::BroadcastUnit { unit, .. } => {
                        if unit.index() == my_shard_index {
                            break unit;
                        }
                    }
                    StateManagerToCore::Event(event) => {
                        eprintln!("Received event: {:?}", event);
                    }
                    _ => {}
                }
            }
            _ = &mut timeout => {
                panic!("Timeout waiting for reconstructed unit broadcast");
            }
        }
    };

    // Drop the validator_tx to allow the task to complete
    drop(validator_tx);
    let _ = task_handle.await;

    // 7. Verify the reconstructed unit has the correct signature
    assert_eq!(
        reconstructed_unit,
        all_units[my_shard_index.0 as usize].clone(),
        "Reconstructed unit should match the original unit"
    );
}
