use libp2p::identity::Keypair;

use crate::sharding::{create_units_to_publish, reconstruct_data_shards};
use crate::types::{CommitteeId, ReconstructionError};

const NUM_DATA_SHARDS: usize = 5;
const NUM_CODING_SHARDS: usize = 5;
const MESSAGE_LEN: usize = 103;
const MY_UNIT_INDEX: usize = 2;
const COMMITTEE_ID: CommitteeId = CommitteeId([42u8; 32]);

// Statically assert that MESSAGE_LEN is not divisible by NUM_DATA_SHARDS, to exercise padding.
#[allow(clippy::manual_is_multiple_of)]
const _: () = assert!(MESSAGE_LEN % NUM_DATA_SHARDS != 0);

// TODO(AndrewL): Consolidate all pseudo-random keypair generation into a single function
fn get_keypair(index: u8) -> Keypair {
    let key = [index; 32];
    Keypair::ed25519_from_bytes(key).unwrap()
}

#[test]
fn test_create_units_to_publish_all_units_have_same_signature_and_root() {
    let units = create_units_to_publish(
        vec![42u8; MESSAGE_LEN],
        COMMITTEE_ID,
        get_keypair(0),
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

#[test]
fn test_reconstruct_data_shards_success() {
    let message = vec![42u8; MESSAGE_LEN];
    let units = create_units_to_publish(
        message.clone(),
        COMMITTEE_ID,
        get_keypair(0),
        NUM_DATA_SHARDS,
        NUM_CODING_SHARDS,
    )
    .unwrap();
    let message_root = units[0].root();

    // Pick an arbitrary subset of units (not just the first N).
    let received_indices: Vec<usize> = vec![1, 3, 5, 7, 9];
    let received_units: Vec<_> = received_indices.iter().map(|&i| units[i].clone()).collect();

    let (reconstructed_message, my_shards, proof) = reconstruct_data_shards(
        received_units,
        message_root,
        MY_UNIT_INDEX,
        NUM_DATA_SHARDS,
        NUM_CODING_SHARDS,
    )
    .unwrap();
    assert_eq!(reconstructed_message, message);
    assert_eq!(&my_shards, units[MY_UNIT_INDEX].shards());
    assert!(!proof.siblings.is_empty());
}

#[test]
fn test_reconstruct_data_shards_wrong_root() {
    let units = create_units_to_publish(
        vec![42u8; MESSAGE_LEN],
        COMMITTEE_ID,
        get_keypair(0),
        NUM_DATA_SHARDS,
        NUM_CODING_SHARDS,
    )
    .unwrap();
    let wrong_root = crate::types::MessageRoot([0u8; 32]);
    let received_units: Vec<_> = units.iter().take(NUM_DATA_SHARDS).cloned().collect();
    let result = reconstruct_data_shards(
        received_units,
        wrong_root,
        MY_UNIT_INDEX,
        NUM_DATA_SHARDS,
        NUM_CODING_SHARDS,
    );
    assert!(matches!(result, Err(ReconstructionError::MismatchedMessageRoot)));
}

#[test]
fn test_verify_message_id_signature_rejects_wrong_signature() {
    let keypair_publisher = get_keypair(0);

    let units = create_units_to_publish(
        vec![42u8; MESSAGE_LEN],
        COMMITTEE_ID,
        keypair_publisher.clone(),
        NUM_DATA_SHARDS,
        NUM_CODING_SHARDS,
    )
    .unwrap();
    let message_root = units[0].root();

    let mut tampered_signature = units[0].signature().to_vec();
    tampered_signature[0] ^= 0xFF;

    let public_key = keypair_publisher.public();
    let nonce = units[0].nonce();
    let result = crate::signature::verify_message_id_signature(
        &message_root,
        COMMITTEE_ID,
        nonce,
        &tampered_signature,
        &public_key,
    );
    assert!(result.is_err());
}
