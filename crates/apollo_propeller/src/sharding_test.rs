use libp2p::identity::Keypair;
use rstest::*;

use crate::sharding::{create_units_to_publish, reconstruct_message_from_shards};
use crate::types::{Channel, ReconstructionError};

const NUM_DATA_SHARDS: usize = 5;
const NUM_CODING_SHARDS: usize = 5;
const MESSAGE_LEN: usize = 103;
const MY_SHARD_INDEX: usize = 2;

// TODO(AndrewL): Consolidate all pseudo-random keypair generation into a single function
fn get_keypair(index: u8) -> Keypair {
    let key = [index; 32];
    Keypair::ed25519_from_bytes(key).unwrap()
}

#[fixture]
fn keypair() -> Keypair {
    get_keypair(0)
}

#[fixture]
fn channel() -> Channel {
    Channel(42)
}

#[rstest]
fn test_create_units_to_publish_all_units_have_same_signature_and_root(
    keypair: Keypair,
    channel: Channel,
) {
    let units = create_units_to_publish(
        vec![42u8; MESSAGE_LEN],
        channel,
        keypair,
        NUM_DATA_SHARDS,
        NUM_CODING_SHARDS,
    )
    .unwrap();
    assert_eq!(units.len(), NUM_DATA_SHARDS + NUM_CODING_SHARDS);
    let message_root = units[0].root();
    assert!(units.iter().all(|unit| unit.root() == message_root));
    assert!(units.iter().all(|unit| !unit.signature().is_empty()));
    let signature = units[0].signature();
    assert!(units.iter().all(|unit| unit.signature() == signature));
}

#[rstest]
fn test_reconstruct_message_from_shards_success(keypair: Keypair, channel: Channel) {
    let message = vec![42u8; MESSAGE_LEN];
    let units = create_units_to_publish(
        message.clone(),
        channel,
        keypair,
        NUM_DATA_SHARDS,
        NUM_CODING_SHARDS,
    )
    .unwrap();
    let message_root = units[0].root();

    // Pick an arbitrary subset of shards (not just the first N).
    let received_indices: Vec<usize> = vec![1, 3, 5, 7, 9];
    let received_units: Vec<_> = received_indices.iter().map(|&i| units[i].clone()).collect();

    let (reconstructed_message, my_shard, proof) = reconstruct_message_from_shards(
        received_units,
        message_root,
        MY_SHARD_INDEX,
        NUM_DATA_SHARDS,
        NUM_CODING_SHARDS,
    )
    .unwrap();
    assert_eq!(reconstructed_message, message);
    assert_eq!(my_shard, units[MY_SHARD_INDEX].shard());
    assert!(!proof.siblings.is_empty());
}

#[rstest]
fn test_reconstruct_message_from_shards_wrong_root(keypair: Keypair, channel: Channel) {
    let units = create_units_to_publish(
        vec![42u8; MESSAGE_LEN],
        channel,
        keypair,
        NUM_DATA_SHARDS,
        NUM_CODING_SHARDS,
    )
    .unwrap();
    let wrong_root = crate::types::MessageRoot([0u8; 32]);
    let received_units: Vec<_> = units.iter().take(NUM_DATA_SHARDS).cloned().collect();
    let result = reconstruct_message_from_shards(
        received_units,
        wrong_root,
        MY_SHARD_INDEX,
        NUM_DATA_SHARDS,
        NUM_CODING_SHARDS,
    );
    assert!(matches!(result, Err(ReconstructionError::MismatchedMessageRoot)));
}

#[rstest]
fn test_reconstruct_message_from_shards_wrong_signature(channel: Channel) {
    let keypair_publisher = get_keypair(0);
    let keypair_other = get_keypair(1);

    let units = create_units_to_publish(
        vec![42u8; MESSAGE_LEN],
        channel,
        keypair_publisher,
        NUM_DATA_SHARDS,
        NUM_CODING_SHARDS,
    )
    .unwrap();
    let message_root = units[0].root();

    // Verify the signature against a different keypair's public key - it should fail.
    let received_units: Vec<_> = units.iter().take(NUM_DATA_SHARDS).cloned().collect();
    let (_, _, _) = reconstruct_message_from_shards(
        received_units,
        message_root,
        MY_SHARD_INDEX,
        NUM_DATA_SHARDS,
        NUM_CODING_SHARDS,
    )
    .unwrap();

    // The signature was created by keypair_publisher but verified against keypair_other's
    // public key, demonstrating signature mismatch detection.
    let other_public_key = keypair_other.public();
    let signature = units[0].signature();
    let result =
        crate::signature::verify_message_id_signature(&message_root, signature, &other_public_key);
    assert!(result.is_err());
}
