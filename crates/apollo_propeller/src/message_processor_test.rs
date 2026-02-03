//! Tests for message processor.

use std::sync::Arc;

use libp2p::identity::Keypair;
use libp2p::PeerId;
use tokio::sync::mpsc;

use crate::config::Config;
use crate::message_processor::{MessageProcessor, StateManagerToEngine};
use crate::sharding::prepare_units;
use crate::tree::PropellerScheduleManager;
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

    let publisher_public_key = publisher_keypair.public();

    let all_units = prepare_units(
        channel,
        publisher,
        publisher_keypair,
        message.clone(),
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
    let tree_manager = PropellerScheduleManager::new(local_peer_id, peer_weights)
        .expect("Failed to create schedule manager");
    let tree_manager = Arc::new(tree_manager);

    // Determine which shard index we (local_peer_id) are responsible for
    let my_shard_index =
        tree_manager.get_my_shard_index(&publisher).expect("Should have a shard index");

    // Get the message root from the units
    let message_root = all_units[0].root();

    // 5. Set up channels for message processor
    let (unit_tx, unit_rx) = mpsc::unbounded_channel();
    let (engine_tx, mut engine_rx) = mpsc::unbounded_channel();

    let config = Config::default();

    // Create and spawn the message processor
    let processor = MessageProcessor {
        channel,
        publisher,
        message_root,
        my_shard_index,
        publisher_public_key,
        tree_manager: tree_manager.clone(),
        local_peer_id,
        unit_rx,
        engine_tx,
        timeout: config.stale_message_timeout,
    };

    let task_handle = tokio::spawn(processor.run());

    // 6. Send shards to the message processor (all except the one we're responsible for)
    // Each shard should come from its designated broadcaster (peer who owns that shard)
    for (i, unit) in all_units.iter().enumerate() {
        let shard_index = ShardIndex(i.try_into().unwrap());
        if shard_index != my_shard_index {
            // Get the peer who is responsible for broadcasting this shard
            let expected_broadcaster =
                tree_manager.get_peer_for_shard_index(&publisher, shard_index).unwrap();
            unit_tx.send((expected_broadcaster, unit.clone())).expect("Failed to send unit");
        }
    }

    // Wait for the message processor to broadcast the reconstructed unit (with timeout)
    let timeout_duration = std::time::Duration::from_secs(5);
    let timeout = tokio::time::sleep(timeout_duration);
    tokio::pin!(timeout);

    let reconstructed_unit = loop {
        tokio::select! {
            Some(msg) = engine_rx.recv() => {
                match msg {
                    StateManagerToEngine::BroadcastUnit { unit, .. } => {
                        if unit.index() == my_shard_index {
                            break unit;
                        }
                    }
                    StateManagerToEngine::Event(event) => {
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

    // Drop the unit_tx to allow the task to complete
    drop(unit_tx);
    let _ = task_handle.await;

    // 7. Verify the reconstructed unit has the correct signature
    let index: usize = my_shard_index.0.try_into().unwrap();
    assert_eq!(
        reconstructed_unit,
        all_units[index].clone(),
        "Reconstructed unit should match the original unit"
    );
}
