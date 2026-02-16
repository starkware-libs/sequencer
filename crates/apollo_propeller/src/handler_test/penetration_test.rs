/// Adversarial / penetration tests for the Propeller handler.
///
/// These tests simulate a malicious remote peer that deliberately crafts pathological inputs
/// to probe for crashes, panics, resource exhaustion, state corruption, and protocol
/// violations in the Handler. The handler must remain operational (no panics, no unbounded
/// allocations, no broken state) under all of these scenarios.
use apollo_protobuf::protobuf::{
    Hash256 as ProtoHash256,
    MerkleProof as ProtoMerkleProof,
    PeerId as ProtoPeerId,
    PropellerUnit as ProtoUnit,
    PropellerUnitBatch as ProtoBatch,
    Shard as ProtoShard,
    ShardsOfPeer as ProtoShardsOfPeer,
};
use asynchronous_codec::Framed;
use futures::prelude::*;
use libp2p::swarm::handler::{
    ConnectionEvent,
    ConnectionHandler,
    FullyNegotiatedInbound,
    StreamUpgradeError,
};
use prost::Message;

use super::framework::*;
use crate::protocol::PropellerCodec;

// =========================================================================
// 1. Malformed protobuf payloads
// =========================================================================

#[tokio::test]
async fn garbage_bytes_after_valid_length_prefix() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound, mut remote, _h) = get_connected_streams().await;
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    // Varint length = 10, followed by 10 bytes of 0xDE (not valid protobuf)
    let mut payload = vec![10u8]; // varint for 10
    payload.extend_from_slice(&[0xDE; 10]);
    remote_send_raw_bytes(&mut remote, &payload).await;

    // The handler should close the inbound substream (decode error), never panic
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    for _ in 0..10 {
        if handler.next().now_or_never().is_none() {
            break;
        }
    }
    validate_no_events(&mut handler);
}

#[tokio::test]
async fn zero_length_message() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound, mut remote, _h) = get_connected_streams().await;
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    // Length prefix = 0: an empty protobuf message
    remote_send_raw_bytes(&mut remote, &[0x00]).await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    // An empty ProtoBatch has an empty `batch` Vec — the handler should process it with no events
    for _ in 0..10 {
        if handler.next().now_or_never().is_none() {
            break;
        }
    }
    validate_no_events(&mut handler);
}

#[tokio::test]
async fn varint_overflow_attack() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound, mut remote, _h) = get_connected_streams().await;
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    // 10-byte varint that overflows u64: all continuation bytes
    let malicious_varint = [0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x02];
    remote_send_raw_bytes(&mut remote, &malicious_varint).await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    for _ in 0..10 {
        if handler.next().now_or_never().is_none() {
            break;
        }
    }
    validate_no_events(&mut handler);
}

#[tokio::test]
async fn length_prefix_claims_max_u32_bytes() {
    let (mut handler, mut _unit_rx) = make_handler_with_max_size(1024);
    let (inbound, mut remote, _h) = get_connected_streams().await;

    let framed = Framed::new(inbound, PropellerCodec::new(1024));
    handler.on_connection_event(ConnectionEvent::FullyNegotiatedInbound(FullyNegotiatedInbound {
        protocol: framed,
        info: (),
    }));

    // Varint for 0xFFFFFFFF (4294967295) — far exceeds max_wire_message_size
    let huge_varint = [0xFF, 0xFF, 0xFF, 0xFF, 0x0F];
    remote_send_raw_bytes(&mut remote, &huge_varint).await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    for _ in 0..10 {
        if handler.next().now_or_never().is_none() {
            break;
        }
    }
    // Handler must reject via codec max-size check, not allocate 4 GiB
    validate_no_events(&mut handler);
}

// =========================================================================
// 2. Semantic protobuf attacks (valid wire encoding, malicious content)
// =========================================================================

fn craft_raw_batch(units: Vec<ProtoUnit>) -> Vec<u8> {
    let batch = ProtoBatch { batch: units };
    let mut buf = Vec::new();
    batch.encode_length_delimited(&mut buf).unwrap();
    buf
}

#[tokio::test]
async fn batch_with_all_fields_missing() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound, mut remote, _h) = get_connected_streams().await;
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    // Unit where every optional field is None and every Vec is empty
    let empty_unit = ProtoUnit {
        shards: None,
        index: 0,
        merkle_root: None,
        merkle_proof: None,
        publisher: None,
        signature: vec![],
        committee_id: None,
        nonce: 0,
    };
    let buf = craft_raw_batch(vec![empty_unit]);
    remote_send_raw_bytes(&mut remote, &buf).await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    // PropellerUnit::try_from should fail on missing fields — handler should warn, not panic
    for _ in 0..10 {
        if handler.next().now_or_never().is_none() {
            break;
        }
    }
    validate_no_events(&mut handler);
}

#[tokio::test]
async fn batch_with_invalid_peer_id() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound, mut remote, _h) = get_connected_streams().await;
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    let bad_unit = ProtoUnit {
        shards: Some(ProtoShardsOfPeer { shards: vec![ProtoShard { data: vec![1, 2, 3] }] }),
        index: 0,
        merkle_root: Some(ProtoHash256 { elements: vec![42u8; 32] }),
        merkle_proof: Some(ProtoMerkleProof {
            siblings: vec![ProtoHash256 { elements: vec![0u8; 32] }],
        }),
        publisher: Some(ProtoPeerId { id: vec![0xFF; 5] }), // invalid multihash
        signature: vec![0u8; 64],
        committee_id: Some(ProtoHash256 { elements: vec![1u8; 32] }),
        nonce: 0,
    };
    let buf = craft_raw_batch(vec![bad_unit]);
    remote_send_raw_bytes(&mut remote, &buf).await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    for _ in 0..10 {
        if handler.next().now_or_never().is_none() {
            break;
        }
    }
    // Invalid PeerId should cause try_from to fail, not panic
    validate_no_events(&mut handler);
}

#[tokio::test]
async fn batch_with_wrong_sized_merkle_root() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound, mut remote, _h) = get_connected_streams().await;
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    let bad_unit = ProtoUnit {
        shards: Some(ProtoShardsOfPeer { shards: vec![ProtoShard { data: vec![1] }] }),
        index: 0,
        merkle_root: Some(ProtoHash256 { elements: vec![42u8; 31] }), // 31 bytes, not 32
        merkle_proof: Some(ProtoMerkleProof {
            siblings: vec![ProtoHash256 { elements: vec![0u8; 32] }],
        }),
        publisher: Some(ProtoPeerId { id: libp2p::PeerId::random().to_bytes() }),
        signature: vec![0u8; 64],
        committee_id: Some(ProtoHash256 { elements: vec![1u8; 32] }),
        nonce: 0,
    };
    let buf = craft_raw_batch(vec![bad_unit]);
    remote_send_raw_bytes(&mut remote, &buf).await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    for _ in 0..10 {
        if handler.next().now_or_never().is_none() {
            break;
        }
    }
    validate_no_events(&mut handler);
}

#[tokio::test]
async fn batch_with_wrong_sized_merkle_siblings() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound, mut remote, _h) = get_connected_streams().await;
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    let bad_unit = ProtoUnit {
        shards: Some(ProtoShardsOfPeer { shards: vec![ProtoShard { data: vec![1] }] }),
        index: 0,
        merkle_root: Some(ProtoHash256 { elements: vec![42u8; 32] }),
        merkle_proof: Some(ProtoMerkleProof {
            siblings: vec![
                ProtoHash256 { elements: vec![0u8; 32] },
                ProtoHash256 { elements: vec![0u8; 7] }, // wrong size
            ],
        }),
        publisher: Some(ProtoPeerId { id: libp2p::PeerId::random().to_bytes() }),
        signature: vec![0u8; 64],
        committee_id: Some(ProtoHash256 { elements: vec![1u8; 32] }),
        nonce: 0,
    };
    let buf = craft_raw_batch(vec![bad_unit]);
    remote_send_raw_bytes(&mut remote, &buf).await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    for _ in 0..10 {
        if handler.next().now_or_never().is_none() {
            break;
        }
    }
    validate_no_events(&mut handler);
}

#[tokio::test]
async fn batch_with_index_exceeding_u32() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound, mut remote, _h) = get_connected_streams().await;
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    // ShardIndex wraps u64, so u64::MAX is a valid (extreme) index — the unit is ACCEPTED.
    // This test verifies no panic on extreme index values and confirms delivery to the channel.
    let bad_unit = ProtoUnit {
        shards: Some(ProtoShardsOfPeer { shards: vec![ProtoShard { data: vec![1, 2] }] }),
        index: u64::MAX,
        merkle_root: Some(ProtoHash256 { elements: vec![42u8; 32] }),
        merkle_proof: Some(ProtoMerkleProof {
            siblings: vec![ProtoHash256 { elements: vec![0u8; 32] }],
        }),
        publisher: Some(ProtoPeerId { id: libp2p::PeerId::random().to_bytes() }),
        signature: vec![0u8; 64],
        committee_id: Some(ProtoHash256 { elements: vec![1u8; 32] }),
        nonce: 0,
    };
    let buf = craft_raw_batch(vec![bad_unit]);
    remote_send_raw_bytes(&mut remote, &buf).await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    // Drive handler to process the unit
    for _ in 0..10 {
        if handler.next().now_or_never().is_none() {
            break;
        }
    }
    // Unit should be delivered to the channel (accepted, not rejected)
    use futures::StreamExt;
    let received = _unit_rx.next().now_or_never();
    assert!(
        matches!(received, Some(Some(ref u)) if u.index().0 == u64::MAX),
        "Unit with u64::MAX index should be accepted, got: {received:?}"
    );
}

#[tokio::test]
async fn batch_with_huge_index() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound, mut remote, _h) = get_connected_streams().await;
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    // ShardIndex wraps u64, so u32::MAX + 1 is valid — the unit is ACCEPTED.
    // This test verifies no panic on the u32 boundary and confirms delivery to the channel.
    let bad_unit = ProtoUnit {
        shards: Some(ProtoShardsOfPeer { shards: vec![ProtoShard { data: vec![1, 2] }] }),
        index: u64::from(u32::MAX) + 1,
        merkle_root: Some(ProtoHash256 { elements: vec![42u8; 32] }),
        merkle_proof: Some(ProtoMerkleProof {
            siblings: vec![ProtoHash256 { elements: vec![0u8; 32] }],
        }),
        publisher: Some(ProtoPeerId { id: libp2p::PeerId::random().to_bytes() }),
        signature: vec![0u8; 64],
        committee_id: Some(ProtoHash256 { elements: vec![1u8; 32] }),
        nonce: 0,
    };
    let buf = craft_raw_batch(vec![bad_unit]);
    remote_send_raw_bytes(&mut remote, &buf).await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    // Drive handler to process the unit
    for _ in 0..10 {
        if handler.next().now_or_never().is_none() {
            break;
        }
    }
    // Unit should be delivered to the channel (accepted, not rejected)
    use futures::StreamExt;
    let received = _unit_rx.next().now_or_never();
    assert!(
        matches!(received, Some(Some(ref u)) if u.index().0 == u64::from(u32::MAX) + 1),
        "Unit with u32::MAX+1 index should be accepted, got: {received:?}"
    );
}

// =========================================================================
// 3. Resource exhaustion / DoS vectors
// =========================================================================

#[tokio::test]
async fn rapid_inbound_substream_churn() {
    let (mut handler, mut _unit_rx) = make_handler();

    // Open and close 50 inbound substreams in rapid succession
    for _ in 0..50 {
        let (inbound, remote, _h) = get_connected_streams().await;
        simulate_fully_negotiated_inbound(&mut handler, inbound);
        drop(remote);
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        for _ in 0..10 {
            if handler.next().now_or_never().is_none() {
                break;
            }
        }
    }

    // Handler must still be usable after the churn
    let (inbound, remote, _h) = get_connected_streams().await;
    let mut remote_f = remote_framed(remote);
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    let unit = make_test_unit();
    remote_send_units(&mut remote_f, vec![unit.clone()]).await;
    validate_received_unit(&mut handler, &mut _unit_rx, &unit).await;
}

#[tokio::test]
async fn many_messages_flood() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound, remote, _h) = get_connected_streams().await;
    let mut remote_f = remote_framed(remote);
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    // Flood 200 units in a single batch
    let units: Vec<_> = (0..200u8).map(|i| make_test_unit_with_shard(vec![i; 20])).collect();
    remote_send_units(&mut remote_f, units.clone()).await;

    // Handler should deliver all 200 — no silent drops, no panics
    for unit in &units {
        validate_received_unit(&mut handler, &mut _unit_rx, unit).await;
    }
    validate_no_events(&mut handler);
}

#[tokio::test]
async fn send_queue_flood() {
    let (mut handler, mut _unit_rx) = make_handler();

    // Queue 500 outbound units before any substream exists
    for i in 0..500u16 {
        simulate_send_unit(
            &mut handler,
            make_test_unit_with_shard(vec![u8::try_from(i & 0xFF).unwrap(); 20]),
        );
    }

    // Handler should still request a substream (not panic or wedge)
    validate_outbound_substream_request(&mut handler).await;
}

#[tokio::test]
async fn batch_with_many_empty_units() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound, mut remote, _h) = get_connected_streams().await;
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    // 1000 completely-empty proto units in one batch — all will fail try_from
    let empty_units: Vec<ProtoUnit> = (0..1000)
        .map(|_| ProtoUnit {
            shards: None,
            index: 0,
            merkle_root: None,
            merkle_proof: None,
            publisher: None,
            signature: vec![],
            committee_id: None,
            nonce: 0,
        })
        .collect();
    let buf = craft_raw_batch(empty_units);
    remote_send_raw_bytes(&mut remote, &buf).await;

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    // All 1000 should be silently dropped (try_from fails), never panic
    for _ in 0..10 {
        if handler.next().now_or_never().is_none() {
            break;
        }
    }
    validate_no_events(&mut handler);
}

// =========================================================================
// 4. Connection lifecycle attacks
// =========================================================================

#[tokio::test]
async fn immediate_close_after_negotiation() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound, remote, _h) = get_connected_streams().await;
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    // Immediately drop the remote (TCP RST equivalent)
    drop(remote);

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    for _ in 0..10 {
        if handler.next().now_or_never().is_none() {
            break;
        }
    }
    validate_no_events(&mut handler);
}

#[tokio::test]
async fn repeated_dial_upgrade_errors_with_full_queue() {
    let (mut handler, mut _unit_rx) = make_handler();

    // Fill the queue with 20 messages, trigger substream request, then error.
    // The first error drains the queue and emits a SendError. Subsequent cycles
    // each queue a fresh message, request, error, drain, and emit SendError.
    for _ in 0..20 {
        simulate_send_unit(&mut handler, make_test_unit());
    }
    validate_outbound_substream_request(&mut handler).await;

    // First error drains all 20 queued messages
    simulate_dial_upgrade_error(&mut handler, 0, StreamUpgradeError::Timeout);
    validate_send_error(&mut handler).await;

    // Now cycle: enqueue one, request, error, drain — 100 times
    for _ in 0..100 {
        simulate_send_unit(&mut handler, make_test_unit());
        validate_outbound_substream_request(&mut handler).await;
        simulate_dial_upgrade_error(&mut handler, 0, StreamUpgradeError::Timeout);
        validate_send_error(&mut handler).await;
    }

    // Handler must still be responsive
    validate_no_events(&mut handler);
}

#[tokio::test]
async fn mixed_negotiation_errors_cycle() {
    let (mut handler, mut _unit_rx) = make_handler();

    let errors = [
        StreamUpgradeError::Timeout,
        StreamUpgradeError::NegotiationFailed,
        StreamUpgradeError::Io(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "broken")),
    ];

    // Each iteration: enqueue a message, request substream, error drains queue + emits SendError
    for error in errors {
        simulate_send_unit(&mut handler, make_test_unit());
        validate_outbound_substream_request(&mut handler).await;
        simulate_dial_upgrade_error(&mut handler, 0, error);
        validate_send_error(&mut handler).await;
    }
}

// =========================================================================
// 5. Wire framing edge cases
// =========================================================================

#[tokio::test]
async fn multiple_messages_in_single_tcp_write() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound, mut remote, _h) = get_connected_streams().await;
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    let unit1 = make_test_unit_with_shard(vec![0xAA; 10]);
    let unit2 = make_test_unit_with_shard(vec![0xBB; 10]);

    // Encode two separate length-delimited batches into a single byte buffer
    let mut combined = Vec::new();
    let batch1 = ProtoBatch { batch: vec![ProtoUnit::from(unit1.clone())] };
    batch1.encode_length_delimited(&mut combined).unwrap();
    let batch2 = ProtoBatch { batch: vec![ProtoUnit::from(unit2.clone())] };
    batch2.encode_length_delimited(&mut combined).unwrap();

    // Send both in one write — the codec must correctly parse both frames
    remote_send_raw_bytes(&mut remote, &combined).await;

    validate_received_unit(&mut handler, &mut _unit_rx, &unit1).await;
    validate_received_unit(&mut handler, &mut _unit_rx, &unit2).await;
    validate_no_events(&mut handler);
}

#[tokio::test]
async fn byte_at_a_time_delivery() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound, mut remote, _h) = get_connected_streams().await;
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    // Encode a valid message
    let unit = make_test_unit_with_shard(vec![0xCC; 15]);
    let batch = ProtoBatch { batch: vec![ProtoUnit::from(unit.clone())] };
    let mut buf = Vec::new();
    batch.encode_length_delimited(&mut buf).unwrap();

    // Drip-feed byte by byte
    for byte in &buf {
        remote_send_raw_bytes(&mut remote, std::slice::from_ref(byte)).await;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }

    // Despite byte-by-byte delivery, the codec should reassemble correctly
    validate_received_unit(&mut handler, &mut _unit_rx, &unit).await;
    validate_no_events(&mut handler);
}

#[tokio::test]
async fn valid_message_after_garbage() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound, mut remote, _h) = get_connected_streams().await;
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    // Send garbage first
    remote_send_raw_bytes(&mut remote, &[0xFF, 0xFF, 0xFF, 0xFF, 0x7F]).await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    // Drain the read-error close cycle
    for _ in 0..10 {
        if handler.next().now_or_never().is_none() {
            break;
        }
    }

    // After the garbage kills the first substream, open a fresh one and send valid data
    let (inbound2, remote2, _h2) = get_connected_streams().await;
    let mut remote2_f = remote_framed(remote2);
    simulate_fully_negotiated_inbound(&mut handler, inbound2);

    let unit = make_test_unit();
    remote_send_units(&mut remote2_f, vec![unit.clone()]).await;
    validate_received_unit(&mut handler, &mut _unit_rx, &unit).await;
}

// =========================================================================
// 6. State machine integrity
// =========================================================================

#[tokio::test]
async fn simultaneous_inbound_and_outbound_traffic() {
    let (mut handler, mut _unit_rx) = make_handler();

    // Set up inbound
    let (inbound, remote_in, _h1) = get_connected_streams().await;
    let mut remote_in_f = remote_framed(remote_in);
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    // Set up outbound
    let outbound_unit = make_test_unit_with_shard(vec![0xDD; 20]);
    simulate_send_unit(&mut handler, outbound_unit.clone());
    validate_outbound_substream_request(&mut handler).await;

    let (outbound, remote_out, _h2) = get_connected_streams().await;
    let mut remote_out_f = remote_framed(remote_out);
    simulate_fully_negotiated_outbound(&mut handler, outbound, 0);

    // Send inbound data at the same time
    let inbound_unit = make_test_unit_with_shard(vec![0xEE; 20]);
    remote_send_units(&mut remote_in_f, vec![inbound_unit.clone()]).await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Drive the handler so it processes inbound data
    for _ in 0..20 {
        match handler.next().now_or_never() {
            Some(Some(_)) => continue,
            _ => break,
        }
    }
    // The inbound unit should arrive via the channel
    use futures::StreamExt;
    let received = _unit_rx.next().now_or_never();
    assert!(
        matches!(received, Some(Some(ref u)) if u == &inbound_unit),
        "Inbound unit should be received during simultaneous I/O"
    );

    // Also verify the outbound data actually arrived at the remote
    let batch = tokio::select! {
        batch = remote_recv_batch(&mut remote_out_f) => batch,
        _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {
            // If we didn't receive yet, poll the handler more and try again
            for _ in 0..10 {
                let _ = handler.next().now_or_never();
            }
            remote_out_f.next().await.unwrap().unwrap()
        }
    };
    assert!(!batch.batch.is_empty(), "Outbound batch should arrive at remote");
}

#[tokio::test]
async fn outbound_after_inbound_error() {
    let (mut handler, mut _unit_rx) = make_handler();

    // Set up and break inbound
    let (inbound, mut remote_raw, _h) = get_connected_streams().await;
    simulate_fully_negotiated_inbound(&mut handler, inbound);
    remote_send_raw_bytes(&mut remote_raw, &[0xFF, 0xFF, 0xFF, 0xFF, 0x7F]).await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    for _ in 0..10 {
        if handler.next().now_or_never().is_none() {
            break;
        }
    }

    // Outbound should still work perfectly despite broken inbound
    let unit = make_test_unit();
    simulate_send_unit(&mut handler, unit.clone());
    validate_outbound_substream_request(&mut handler).await;

    let (outbound, remote_out, _h2) = get_connected_streams().await;
    let mut remote_out_f = remote_framed(remote_out);
    simulate_fully_negotiated_outbound(&mut handler, outbound, 0);

    let batch = tokio::select! {
        batch = remote_recv_batch(&mut remote_out_f) => batch,
        _ = handler.next() => {
            remote_out_f.next().await.unwrap().unwrap()
        }
    };
    assert_eq!(batch.batch.len(), 1);
}

// =========================================================================
// 7. Large field values within wire limit (V6)
// =========================================================================

#[tokio::test]
async fn large_shard_within_wire_limit() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound, remote, _h) = get_connected_streams().await;
    let mut remote_f = remote_framed(remote);
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    // A unit with a ~100 KB shard — well within the 1 MB wire limit but large enough
    // to stress the handler. Proves that large-but-valid individual fields don't cause
    // OOM or panics; the codec accepts any frame within max_wire_message_size.
    // (Kept under yamux's default window size to avoid transport-level deadlocks in tests.)
    let unit = make_test_unit_with_shard(vec![0xAB; 100_000]);
    remote_send_units(&mut remote_f, vec![unit.clone()]).await;

    validate_received_unit(&mut handler, &mut _unit_rx, &unit).await;
}

// =========================================================================
// 8. Idle inbound substream does not block outbound (V19)
// =========================================================================

#[tokio::test]
async fn inbound_idle_does_not_block_outbound() {
    let (mut handler, mut _unit_rx) = make_handler();

    // Occupy the inbound slot but never send data — simulating a peer that
    // negotiates a substream and then goes silent.
    let (inbound, _remote_idle, _h1) = get_connected_streams().await;
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    // Outbound should still work independently
    let unit = make_test_unit();
    simulate_send_unit(&mut handler, unit.clone());
    validate_outbound_substream_request(&mut handler).await;

    let (outbound, remote_out, _h2) = get_connected_streams().await;
    let mut remote_out_f = remote_framed(remote_out);
    simulate_fully_negotiated_outbound(&mut handler, outbound, 0);

    let batch = tokio::select! {
        batch = remote_recv_batch(&mut remote_out_f) => batch,
        _ = handler.next() => {
            remote_out_f.next().await.unwrap().unwrap()
        }
    };
    assert_eq!(batch.batch.len(), 1);
}

// =========================================================================
// 9. Channel receiver dropped — handler survives (V22)
// =========================================================================

#[tokio::test]
async fn channel_closed_handler_survives() {
    let (mut handler, unit_rx) = make_handler();

    // Drop the channel receiver — simulates the engine shutting down.
    drop(unit_rx);

    // Set up inbound and send data. The handler should log a warning
    // and drop the unsent units rather than panicking.
    let (inbound, remote, _h) = get_connected_streams().await;
    let mut remote_f = remote_framed(remote);
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    let unit = make_test_unit();
    remote_send_units(&mut remote_f, vec![unit]).await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    for _ in 0..20 {
        if handler.next().now_or_never().is_none() {
            break;
        }
    }

    // Handler should still be responsive for outbound operations
    simulate_send_unit(&mut handler, make_test_unit());
    validate_outbound_substream_request(&mut handler).await;
}

// =========================================================================
// 10. Empty batch flood — V25
// =========================================================================

/// Sends 100 empty batches (zero-unit protobuf messages) in a single TCP write.
/// The handler must process them without amplification, crash, or excessive CPU.
#[tokio::test]
async fn empty_batch_flood() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound, mut remote, _h) = get_connected_streams().await;
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    // Encode 100 empty batches into a single byte buffer
    let mut combined = Vec::new();
    for _ in 0..100 {
        let empty_batch = ProtoBatch { batch: vec![] };
        empty_batch.encode_length_delimited(&mut combined).unwrap();
    }
    remote_send_raw_bytes(&mut remote, &combined).await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    // Drive handler — should process all empty batches without events or units
    for _ in 0..20 {
        if handler.next().now_or_never().is_none() {
            break;
        }
    }

    // No units should be delivered (all batches were empty)
    use futures::StreamExt;
    assert!(
        _unit_rx.next().now_or_never().is_none(),
        "Empty batches should produce no units"
    );
    validate_no_events(&mut handler);
}

// =========================================================================
// 11. Empty shard units forwarded to engine — V26
// =========================================================================

/// Sends units with structurally valid but empty shard data. The handler should
/// forward them to the engine (structural validation passes, semantic validation
/// is the engine's responsibility).
#[tokio::test]
async fn empty_shard_units_forwarded() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound, remote, _h) = get_connected_streams().await;
    let mut remote_f = remote_framed(remote);
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    // Unit with empty shards vec: ShardsOfPeer(vec![])
    let unit_empty_vec = make_test_unit_with_shard(vec![]);
    // Unit with zero-length shard data
    let unit_zero_len = make_test_unit_with_shard(vec![]);

    remote_send_units(&mut remote_f, vec![unit_empty_vec.clone(), unit_zero_len.clone()]).await;

    // Both should be accepted and forwarded to the channel
    validate_received_unit(&mut handler, &mut _unit_rx, &unit_empty_vec).await;
    validate_received_unit(&mut handler, &mut _unit_rx, &unit_zero_len).await;
    validate_no_events(&mut handler);
}

// =========================================================================
// 12. Spoofed publisher PeerId forwarded unchanged — V27
// =========================================================================

/// A peer sends a unit with an arbitrary publisher PeerId (not matching the
/// connection's actual remote peer). The handler should forward it unchanged —
/// it has no access to the connection's remote PeerId and cannot validate.
#[tokio::test]
async fn spoofed_publisher_forwarded() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound, remote, _h) = get_connected_streams().await;
    let mut remote_f = remote_framed(remote);
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    // Create a unit with a specific "spoofed" publisher PeerId
    let spoofed_peer = libp2p::PeerId::random();
    let unit = crate::PropellerUnit::new(
        crate::types::CommitteeId([1u8; 32]),
        spoofed_peer,
        crate::types::MessageRoot([42u8; 32]),
        vec![0u8; 64],
        crate::types::ShardIndex(0),
        crate::unit::ShardsOfPeer(vec![crate::unit::Shard(vec![1, 2, 3])]),
        crate::MerkleProof { siblings: vec![[0u8; 32]] },
        0,
    );

    remote_send_units(&mut remote_f, vec![unit.clone()]).await;

    // The unit should be forwarded with the spoofed publisher intact
    validate_received_unit(&mut handler, &mut _unit_rx, &unit).await;
}

// =========================================================================
// 13. Merkle proof with many empty siblings — V29
// =========================================================================

/// A peer sends a unit whose merkle proof contains many empty Hash256 entries
/// (0-byte elements instead of 32). Each empty sibling is ~2 bytes of protobuf
/// overhead. Within max_wire_message_size, thousands of empty siblings fit.
/// `MerkleProof::try_from` calls `Vec::with_capacity(len)` before validation,
/// causing a transient allocation spike. The handler must not crash.
#[tokio::test]
async fn merkle_proof_many_empty_siblings_no_crash() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound, mut remote, _h) = get_connected_streams().await;
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    // Build a unit with 10,000 empty merkle siblings — each will fail the 32-byte
    // check in MerkleProof::try_from, but not before Vec::with_capacity(10_000)
    // allocates ~320 KB for the output siblings vector.
    let many_empty_siblings: Vec<ProtoHash256> =
        (0..10_000).map(|_| ProtoHash256 { elements: vec![] }).collect();

    let unit = ProtoUnit {
        shards: Some(ProtoShardsOfPeer { shards: vec![ProtoShard { data: vec![1, 2, 3] }] }),
        index: 0,
        merkle_root: Some(ProtoHash256 { elements: vec![42u8; 32] }),
        merkle_proof: Some(ProtoMerkleProof { siblings: many_empty_siblings }),
        publisher: Some(ProtoPeerId { id: libp2p::PeerId::random().to_bytes() }),
        signature: vec![0u8; 64],
        committee_id: Some(ProtoHash256 { elements: vec![1u8; 32] }),
        nonce: 0,
    };

    let buf = craft_raw_batch(vec![unit]);
    remote_send_raw_bytes(&mut remote, &buf).await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    for _ in 0..10 {
        if handler.next().now_or_never().is_none() {
            break;
        }
    }

    // The unit should be rejected (empty siblings fail 32-byte check), no crash
    use futures::StreamExt;
    assert!(
        _unit_rx.next().now_or_never().is_none(),
        "Unit with empty merkle siblings should be rejected"
    );
    validate_no_events(&mut handler);
}
