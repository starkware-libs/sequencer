use libp2p::identity::Keypair;
use libp2p::PeerId;
use rstest::*;

use crate::sharding::{prepare_units, rebuild_message};
use crate::types::{Channel, ReconstructionError};

#[fixture]
fn keypair() -> Keypair {
    Keypair::generate_ed25519()
}

#[fixture]
fn peer_id(keypair: Keypair) -> PeerId {
    PeerId::from(keypair.public())
}

#[fixture]
fn channel() -> Channel {
    Channel(42)
}

#[rstest]
fn test_prepare_units_with_signature(keypair: Keypair, peer_id: PeerId, channel: Channel) {
    let units = prepare_units(channel, peer_id, keypair, vec![42u8; 100], 5, 3)
        .expect("Failed to prepare units");
    assert_eq!(units.len(), 8);
    let message_root = units[0].root();
    assert!(units.iter().all(|unit| unit.root() == message_root));
    assert!(units.iter().all(|unit| !unit.signature().is_empty()));
    let signature = units[0].signature();
    assert!(units.iter().all(|unit| unit.signature() == signature));
}

#[rstest]
fn test_prepare_units_with_padding(keypair: Keypair, peer_id: PeerId, channel: Channel) {
    let units = prepare_units(channel, peer_id, keypair, vec![1, 2, 3], 2, 1)
        .expect("Failed to prepare units");
    assert_eq!(units.len(), 3);
}

#[rstest]
fn test_rebuild_message_success(keypair: Keypair, peer_id: PeerId, channel: Channel) {
    let message = vec![42u8; 100];
    let num_data_shards = 5;
    let num_coding_shards = 3;
    let my_shard_index = 2;
    let units = prepare_units(
        channel,
        peer_id,
        keypair,
        message.clone(),
        num_data_shards,
        num_coding_shards,
    )
    .expect("Failed to prepare units");
    let message_root = units[0].root();
    let mut received_shards = Vec::new();
    for (i, unit) in units.iter().enumerate() {
        if i != my_shard_index {
            received_shards.push(unit.clone());
        }
        if received_shards.len() >= num_data_shards {
            break;
        }
    }
    let result = rebuild_message(
        received_shards,
        message_root,
        my_shard_index,
        num_data_shards,
        num_coding_shards,
    );
    assert!(result.is_ok());
    let (reconstructed_message, my_shard, proof) = result.unwrap();
    assert_eq!(reconstructed_message, message);
    assert_eq!(my_shard, units[my_shard_index].shard());
    if num_data_shards + num_coding_shards > 1 {
        assert!(!proof.siblings.is_empty());
    }
}

#[rstest]
fn test_rebuild_message_wrong_root(keypair: Keypair, peer_id: PeerId, channel: Channel) {
    let units = prepare_units(channel, peer_id, keypair, vec![42u8; 100], 5, 3)
        .expect("Failed to prepare units");
    let wrong_root = crate::types::MessageRoot([0u8; 32]);
    let received_shards: Vec<_> = units.iter().take(5).cloned().collect();
    let result = rebuild_message(received_shards, wrong_root, 2, 5, 3);
    assert!(matches!(result, Err(ReconstructionError::MismatchedMessageRoot)));
}
