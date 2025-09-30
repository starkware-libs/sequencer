//! Fuzzing tests for shards
//!
//! This module contains deterministic pseudo-random fuzzing tests that corrupt
//! valid shards in various ways to ensure the verification logic properly rejects
//! invalid data.

#![allow(clippy::as_conversions)]

use apollo_propeller::{
    Behaviour,
    Config,
    MessageAuthenticity,
    PropellerMessage,
    ShardValidationError,
};
use libp2p::identity::{Keypair, PeerId};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaChaRng;

/// Apply random corruption to a byte array
fn corrupt_byte(bytes: &mut [u8], seed: u64) -> (usize, u8, u8) {
    let mut rng = ChaChaRng::seed_from_u64(seed);
    let byte_pos = rng.gen_range(0..bytes.len());
    let original_byte = bytes[byte_pos];
    bytes[byte_pos] ^= rng.gen_range(1..=255u8);
    (byte_pos, original_byte, bytes[byte_pos])
}

/// Test configuration and setup data
struct FuzzTestSetup {
    sender: Behaviour,
    receiver: Behaviour,
    receiver_peer_id: PeerId,
    sender_peer_id: PeerId,
    valid_data: Vec<u8>,
}

/// Creates a complete test setup with sender and receiver behaviours
fn create_fuzz_test_setup() -> FuzzTestSetup {
    let config = Config::builder().build();

    let sender_keypair = Keypair::generate_ed25519();
    let sender_peer_id = PeerId::from(sender_keypair.public());
    let mut sender =
        Behaviour::new(MessageAuthenticity::Signed(sender_keypair.clone()), config.clone());

    let receiver_keypair = Keypair::generate_ed25519();
    let receiver_peer_id = PeerId::from(receiver_keypair.public());
    let mut receiver =
        Behaviour::new(MessageAuthenticity::Signed(receiver_keypair.clone()), config.clone());

    sender.set_peers(vec![(sender_peer_id, 2000), (receiver_peer_id, 1000)]).unwrap();
    receiver.set_peers(vec![(sender_peer_id, 2000), (receiver_peer_id, 1000)]).unwrap();

    let data_size: usize = 64;
    let valid_data = (0..data_size).map(|i| (i % 256) as u8).collect::<Vec<u8>>();

    FuzzTestSetup { sender, receiver, receiver_peer_id, sender_peer_id, valid_data }
}

fn run_repeatedly(mut f: impl FnMut(u64)) {
    const ITERATIONS: u64 = 10_000;
    const PRINT_INTERVAL: u64 = ITERATIONS / 100;
    let mut std_rng = rand::rngs::StdRng::seed_from_u64(24601);
    for i in 0..ITERATIONS {
        if i % PRINT_INTERVAL == 0 {
            println!("Progress: {}/{}", i, ITERATIONS);
        }
        let seed = std_rng.gen::<u64>();
        f(seed);
    }
}

#[test]
fn test_deterministic_shard_corruption_fuzzing() {
    let mut setup = create_fuzz_test_setup();

    let messages = setup.sender.prepare_messages(setup.valid_data).unwrap();
    assert_eq!(messages.len(), 1);
    let message = messages.first().unwrap();
    setup.receiver.validate_shard(setup.sender_peer_id, message).unwrap();

    let mut message_bytes = bytes::BytesMut::new();
    message.encode(&mut message_bytes, 1 << 20);
    let message_bytes = message_bytes.freeze().to_vec();

    let mut error_counter = vec![0; 5];

    run_repeatedly(|seed| {
        let corrupted_message = if seed % 71 == 0 {
            PropellerMessage::new(
                message.root(),
                setup.receiver_peer_id,
                message.signature().to_vec(),
                message.index(),
                message.shard().to_vec(),
                message.proof().clone(),
            )
        } else {
            let mut corrupted_bytes = message_bytes.clone();
            // let (byte_pos, original_byte, new_byte) =
            corrupt_byte(&mut corrupted_bytes, seed);
            // println!(
            //     "Corrupted byte position: {}, original byte: {:02x}, new byte: {:02x}",
            //     byte_pos, original_byte, new_byte
            // );

            // Try to decode the corrupted shard
            let mut corrupted_bytes_mut = bytes::BytesMut::from(corrupted_bytes.as_slice());
            let Some(corrupted_message) =
                apollo_propeller::PropellerMessage::decode(&mut corrupted_bytes_mut, 1 << 20)
            else {
                return;
            };
            corrupted_message
        };
        let sender = if seed % 59 == 0 { PeerId::random() } else { setup.sender_peer_id };

        // Validate the shard - it should fail
        match setup.receiver.validate_shard(sender, &corrupted_message) {
            Ok(_) => panic!("CRITICAL: Corrupted message passed validation! Seed: {}", seed,),
            Err(error) => {
                error_counter[match error {
                    ShardValidationError::ReceivedPublishedShard => 0,
                    ShardValidationError::DuplicateShard => unreachable!(
                        "Cache is not being updated, duplicate shard should not be possible"
                    ),
                    ShardValidationError::TreeError(_) => 1,
                    ShardValidationError::UnexpectedSender { .. } => 2,
                    ShardValidationError::SignatureVerificationFailed(_) => 3,
                    ShardValidationError::ProofVerificationFailed => 4,
                }] += 1;
            }
        }
    });
    println!("Error counter: {:?}", error_counter);
    assert!(error_counter.iter().all(|&count| count > 0));
}

/// Generate a random valid shard for testing
pub(crate) fn generate_random_message(seed: u64) -> apollo_propeller::PropellerMessage {
    let mut rng = ChaChaRng::seed_from_u64(seed);
    apollo_propeller::PropellerMessage::random(&mut rng, 1 << 12)
}

#[test]
fn test_encode_decode_roundtrip_fuzzing() {
    run_repeatedly(|seed| {
        let original_message = generate_random_message(seed);
        let mut encoded_bytes = bytes::BytesMut::new();
        original_message.encode(&mut encoded_bytes, 1 << 20);
        let mut decode_buffer = encoded_bytes.clone();
        let Some(decoded_message) =
            apollo_propeller::PropellerMessage::decode(&mut decode_buffer, 1 << 20)
        else {
            panic!("Encode then decode failed! Seed: {}, Original: {:?}", seed, original_message);
        };
        assert_eq!(decoded_message, original_message);
    });
}

#[test]
fn test_encode_decode_roundtrip_with_corruption_fuzzing() {
    run_repeatedly(|seed| {
        let original_message = generate_random_message(seed);
        let mut encoded_bytes = bytes::BytesMut::new();
        original_message.encode(&mut encoded_bytes, 1 << 20);
        let mut our_bytes = encoded_bytes.to_vec();
        let (byte_pos, original_byte, new_byte) = corrupt_byte(&mut our_bytes, seed);
        let mut decode_buffer = bytes::BytesMut::from(our_bytes.as_slice());
        let Some(decoded_message) =
            apollo_propeller::PropellerMessage::decode(&mut decode_buffer, 1 << 20)
        else {
            return;
        };
        assert_ne!(
            decoded_message, original_message,
            "Seed: {}, Byte pos: {}, Original byte: {:?}, New byte: {:?}.\nThis test assumes that \
             every byte in the encoding matters, if this is not the case, why the useless bytes?",
            seed, byte_pos, original_byte, new_byte
        );
    });
}

#[test]
fn test_deterministic_hash_fuzzing() {
    run_repeatedly(|seed| {
        let original_message = generate_random_message(seed);
        let original_message_hash = original_message.hash();

        let mut bytes = bytes::BytesMut::new();
        original_message.encode(&mut bytes, 1 << 20);
        let original_message_bytes = bytes.freeze().to_vec();

        for i in 0..10 {
            let mut our_bytes = original_message_bytes.clone();
            let (byte_pos, original_byte, new_byte) = corrupt_byte(&mut our_bytes, seed + i);
            let mut decode_buffer = bytes::BytesMut::from(our_bytes.as_slice());
            let Some(decoded_message) =
                apollo_propeller::PropellerMessage::decode(&mut decode_buffer, 1 << 20)
            else {
                continue;
            };
            assert_ne!(
                decoded_message.hash(),
                original_message_hash,
                "Seed: {}, Byte pos: {}, Original byte: {:?}, New byte: {:?}",
                seed,
                byte_pos,
                original_byte,
                new_byte
            );
        }
    });
}
