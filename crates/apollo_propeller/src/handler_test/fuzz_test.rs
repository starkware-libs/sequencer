/// Fuzz / property-based tests for the Propeller handler and codec.
///
/// These tests use randomized inputs to exercise the handler and codec with a wide range of
/// values, probing for panics, undefined behaviour, and invariant violations that
/// deterministic tests might miss.
///
/// Key strategies:
///   - Random byte sequences as raw wire input (codec fuzz).
///   - Random field values inside otherwise-valid ProtoUnit structures (deserialization fuzz).
///   - Random batching parameters against `create_message_batch` (batching invariant checks).
///   - Random interleaving of handler operations (state machine fuzz).
use std::collections::VecDeque;

use apollo_protobuf::protobuf::{
    Hash256 as ProtoHash256,
    MerkleProof as ProtoMerkleProof,
    PeerId as ProtoPeerId,
    PropellerUnit as ProtoUnit,
    PropellerUnitBatch as ProtoBatch,
    Shard as ProtoShard,
    ShardsOfPeer as ProtoShardsOfPeer,
};
use futures::prelude::*;
use libp2p::swarm::handler::StreamUpgradeError;
use prost::Message;

use super::framework::*;
use crate::handler::Handler;

// =========================================================================
// Helpers
// =========================================================================

/// Simple deterministic PRNG (xorshift64) — no external deps needed.
struct Rng(u64);

#[allow(clippy::as_conversions)]
impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn next_usize(&mut self, max: usize) -> usize {
        (self.next_u64() as usize) % max.max(1)
    }

    fn next_bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next_u64() as u8).collect()
    }

    fn next_bool(&mut self) -> bool {
        self.next_u64() & 1 == 0
    }
}

fn craft_raw_batch(units: Vec<ProtoUnit>) -> Vec<u8> {
    let batch = ProtoBatch { batch: units };
    let mut buf = Vec::new();
    batch.encode_length_delimited(&mut buf).unwrap();
    buf
}

// =========================================================================
// 1. Raw codec fuzzing — random bytes fed directly to the handler
// =========================================================================

#[tokio::test]
async fn fuzz_random_bytes_to_codec() {
    // Feed 100 random byte sequences to the handler via a raw stream.
    // The handler must never panic regardless of input.
    let mut rng = Rng::new(0xDEAD_BEEF_CAFE_1234);

    for _ in 0..100 {
        let (mut handler, mut _unit_rx) = make_handler();
        let (inbound, mut remote, _h) = get_connected_streams().await;
        simulate_fully_negotiated_inbound(&mut handler, inbound);

        let len = rng.next_usize(512) + 1;
        let bytes = rng.next_bytes(len);
        remote_send_raw_bytes(&mut remote, &bytes).await;

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        for _ in 0..10 {
            if handler.next().now_or_never().is_none() {
                break;
            }
        }
        // No assertion on specific outcome — just must not panic
    }
}

#[tokio::test]
async fn fuzz_random_varint_prefixes() {
    // Generate random varint-like byte sequences (high-bit continuation bytes)
    let mut rng = Rng::new(0x1234_5678_ABCD_EF00);

    for _ in 0..50 {
        let (mut handler, mut _unit_rx) = make_handler();
        let (inbound, mut remote, _h) = get_connected_streams().await;
        simulate_fully_negotiated_inbound(&mut handler, inbound);

        let varint_len = rng.next_usize(10) + 1;
        #[allow(clippy::as_conversions)]
        let mut bytes: Vec<u8> = (0..varint_len).map(|_| 0x80 | (rng.next_u64() as u8)).collect();
        // Terminate the last byte (clear continuation bit) sometimes
        if rng.next_bool() {
            if let Some(last) = bytes.last_mut() {
                *last &= 0x7F;
            }
        }
        // Optionally append some payload bytes
        let extra = rng.next_usize(64);
        bytes.extend(rng.next_bytes(extra));

        remote_send_raw_bytes(&mut remote, &bytes).await;
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        for _ in 0..10 {
            if handler.next().now_or_never().is_none() {
                break;
            }
        }
    }
}

// =========================================================================
// 2. Protobuf deserialization fuzzing — random field values in ProtoUnit
// =========================================================================

fn random_proto_unit(rng: &mut Rng) -> ProtoUnit {
    let shard_len = rng.next_usize(256);
    let sig_len = rng.next_usize(128);
    let root_len = rng.next_usize(64); // might not be 32
    let num_siblings = rng.next_usize(16);
    let publisher_len = rng.next_usize(64);

    ProtoUnit {
        shards: if rng.next_bool() {
            Some(ProtoShardsOfPeer { shards: vec![ProtoShard { data: rng.next_bytes(shard_len) }] })
        } else {
            None
        },
        index: rng.next_u64(),
        merkle_root: if rng.next_bool() {
            Some(ProtoHash256 { elements: rng.next_bytes(root_len) })
        } else {
            None
        },
        merkle_proof: if rng.next_bool() {
            Some(ProtoMerkleProof {
                siblings: (0..num_siblings)
                    .map(|_| {
                        let sib_len = rng.next_usize(48);
                        ProtoHash256 { elements: rng.next_bytes(sib_len) }
                    })
                    .collect(),
            })
        } else {
            None
        },
        publisher: if rng.next_bool() {
            Some(ProtoPeerId { id: rng.next_bytes(publisher_len) })
        } else {
            None
        },
        signature: rng.next_bytes(sig_len),
        committee_id: if rng.next_bool() {
            Some(ProtoHash256 { elements: rng.next_bytes(32) })
        } else {
            None
        },
        nonce: rng.next_u64(),
    }
}

#[tokio::test]
async fn fuzz_random_proto_units() {
    // Send batches of random ProtoUnits through the handler.
    // Valid ones should be delivered, invalid ones should be dropped — never panic.
    let mut rng = Rng::new(0xFEED_FACE_0000_0001);

    for _ in 0..50 {
        let (mut handler, mut _unit_rx) = make_handler();
        let (inbound, mut remote, _h) = get_connected_streams().await;
        simulate_fully_negotiated_inbound(&mut handler, inbound);

        let num_units = rng.next_usize(20) + 1;
        let units: Vec<ProtoUnit> = (0..num_units).map(|_| random_proto_unit(&mut rng)).collect();
        let buf = craft_raw_batch(units);
        remote_send_raw_bytes(&mut remote, &buf).await;

        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        // Drain whatever events come out
        for _ in 0..100 {
            if handler.next().now_or_never().is_none() {
                break;
            }
        }
    }
}

#[tokio::test]
async fn fuzz_mixed_valid_and_random_units() {
    let mut rng = Rng::new(0xAAAA_BBBB_CCCC_DDDD);

    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound, mut remote, _h) = get_connected_streams().await;
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    let mut expected_valid = Vec::new();

    for _ in 0..30 {
        let mut batch_units = Vec::new();

        // Interleave valid and random units
        for _ in 0..rng.next_usize(8) + 1 {
            if rng.next_bool() {
                let shard_len = rng.next_usize(50) + 1;
                let valid = make_test_unit_with_shard(rng.next_bytes(shard_len));
                batch_units.push(ProtoUnit::from(valid.clone()));
                expected_valid.push(valid);
            } else {
                batch_units.push(random_proto_unit(&mut rng));
            }
        }

        let buf = craft_raw_batch(batch_units);
        remote_send_raw_bytes(&mut remote, &buf).await;
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Drive handler to process all events
    while let Some(Some(_)) = handler.next().now_or_never() {}
    // Drain units from the channel
    use futures::StreamExt;
    let mut received_units = Vec::new();
    while let Some(Some(u)) = _unit_rx.next().now_or_never() {
        received_units.push(u);
    }

    // Every valid unit should have been delivered
    assert_eq!(
        received_units.len(),
        expected_valid.len(),
        "Expected {} valid units but received {}",
        expected_valid.len(),
        received_units.len()
    );
    for (received, expected) in received_units.iter().zip(expected_valid.iter()) {
        assert_eq!(received, expected);
    }
}

// =========================================================================
// 3. Batching invariant fuzzing — create_message_batch properties
// =========================================================================

#[test]
fn fuzz_batch_preserves_total_message_count() {
    let mut rng = Rng::new(0x0BAD_F00D_DEAD_BEEF);

    for _ in 0..200 {
        let num_msgs = rng.next_usize(50) + 1;
        let max_size = rng.next_usize(10_000) + 1;

        let mut queue: VecDeque<ProtoUnit> = (0..num_msgs)
            .map(|_| {
                let shard_len = rng.next_usize(200) + 1;
                ProtoUnit::from(make_test_unit_with_shard(rng.next_bytes(shard_len)))
            })
            .collect();

        let original_count = queue.len();
        let mut total_batched = 0;

        // Drain the queue through create_message_batch
        while !queue.is_empty() {
            let batch = Handler::create_message_batch(&mut queue, max_size);
            assert!(!batch.batch.is_empty(), "Non-empty queue must produce non-empty batch");
            total_batched += batch.batch.len();
        }

        assert_eq!(
            total_batched, original_count,
            "Total batched messages must equal original queue size"
        );
    }
}

#[test]
fn fuzz_batch_first_message_always_included() {
    let mut rng = Rng::new(0xCAFE_BABE_1337_7331);

    for _ in 0..100 {
        let shard_len = rng.next_usize(2000) + 1;
        let max_size = rng.next_usize(100) + 1; // potentially much smaller than the message

        let mut queue = VecDeque::new();
        queue.push_back(ProtoUnit::from(make_test_unit_with_shard(rng.next_bytes(shard_len))));

        let batch = Handler::create_message_batch(&mut queue, max_size);
        assert_eq!(batch.batch.len(), 1, "First message must always be included");
        assert!(queue.is_empty(), "Queue must be empty after taking single message");
    }
}

#[test]
fn fuzz_batch_respects_size_limit() {
    let mut rng = Rng::new(0x5EED_5EED_5EED_5EED);

    for _ in 0..200 {
        let num_msgs = rng.next_usize(30) + 2;
        let max_size = rng.next_usize(5_000) + 50;

        let mut queue: VecDeque<ProtoUnit> = (0..num_msgs)
            .map(|_| {
                let shard_len = rng.next_usize(100) + 1;
                ProtoUnit::from(make_test_unit_with_shard(rng.next_bytes(shard_len)))
            })
            .collect();

        let batch = Handler::create_message_batch(&mut queue, max_size);

        // If the batch has more than one message, it should fit within the limit
        // (only exception: the first message is always included even if oversized)
        if batch.batch.len() > 1 {
            assert!(
                batch.encoded_len() <= max_size,
                "Multi-message batch (size={}) must not exceed max_size ({})",
                batch.encoded_len(),
                max_size
            );
        }
    }
}

// =========================================================================
// 4. State machine fuzzing — random operation sequences on the handler
// =========================================================================

#[tokio::test]
async fn fuzz_random_handler_operations() {
    // Execute random sequences of handler operations. The handler must never panic.
    let mut rng = Rng::new(0xDEAD_C0DE_FACE_B00C);

    for _ in 0..20 {
        let (mut handler, mut _unit_rx) = make_handler();
        let num_ops = rng.next_usize(30) + 5;

        // Keep track of live streams so they don't get dropped prematurely
        let mut _handles = Vec::new();

        for _ in 0..num_ops {
            match rng.next_usize(6) {
                0 => {
                    // Simulate inbound substream
                    let (inbound, _remote, h) = get_connected_streams().await;
                    _handles.push(h);
                    simulate_fully_negotiated_inbound(&mut handler, inbound);
                }
                1 => {
                    // Queue an outbound unit
                    let shard_len = rng.next_usize(50) + 1;
                    let shard = rng.next_bytes(shard_len);
                    simulate_send_unit(&mut handler, make_test_unit_with_shard(shard));
                }
                2 => {
                    // Simulate outbound negotiation
                    let (outbound, _remote, h) = get_connected_streams().await;
                    _handles.push(h);
                    simulate_fully_negotiated_outbound(&mut handler, outbound, 0);
                }
                3 => {
                    // Simulate DialUpgradeError
                    let error_type = match rng.next_usize(3) {
                        0 => StreamUpgradeError::Timeout,
                        1 => StreamUpgradeError::NegotiationFailed,
                        _ => StreamUpgradeError::Io(std::io::Error::other("fuzz error")),
                    };
                    simulate_dial_upgrade_error(&mut handler, 0, error_type);
                }
                4 => {
                    // Poll the handler
                    for _ in 0..5 {
                        if handler.next().now_or_never().is_none() {
                            break;
                        }
                    }
                }
                _ => {
                    // Small delay to let async operations settle
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
            }
        }

        // Final drain
        for _ in 0..20 {
            if handler.next().now_or_never().is_none() {
                break;
            }
        }
    }
}

#[tokio::test]
async fn fuzz_alternating_inbound_errors_and_recovery() {
    let mut rng = Rng::new(0xBAAD_F00D_1111_2222);

    let (mut handler, mut _unit_rx) = make_handler();

    for i in 0..30 {
        let (inbound, mut remote, _h) = get_connected_streams().await;
        simulate_fully_negotiated_inbound(&mut handler, inbound);

        if rng.next_bool() {
            // Send garbage
            let garbage_len = rng.next_usize(100) + 1;
            let garbage = rng.next_bytes(garbage_len);
            remote_send_raw_bytes(&mut remote, &garbage).await;
        } else {
            // Send valid data
            let unit = make_test_unit_with_shard(vec![u8::try_from(i).unwrap(); 10]);
            let mut remote_f = remote_framed(remote);
            remote_send_units(&mut remote_f, vec![unit]).await;
        }

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        for _ in 0..15 {
            if handler.next().now_or_never().is_none() {
                break;
            }
        }
    }

    // Drain any leftover units from earlier cycles
    while _unit_rx.try_next().is_ok() {}

    // Handler should still be alive
    let (inbound, remote, _h) = get_connected_streams().await;
    let mut remote_f = remote_framed(remote);
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    let final_unit = make_test_unit();
    remote_send_units(&mut remote_f, vec![final_unit.clone()]).await;
    validate_received_unit(&mut handler, &mut _unit_rx, &final_unit).await;
}
