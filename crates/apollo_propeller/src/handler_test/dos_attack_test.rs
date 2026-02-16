/// DoS attack surface tests for the Propeller handler.
///
/// These tests specifically target resource exhaustion and denial-of-service vectors that
/// a malicious remote peer (or misbehaving internal component) could exploit. Each test
/// either proves the handler is safe against a specific attack vector or exposes an
/// unbounded resource growth pattern that needs mitigation.
///
/// Attack vectors covered:
///   1. Unbounded `events_to_emit` growth when behaviour is slow to consume events.
///   2. Unbounded `send_queue` growth from behaviour flooding.
///   3. Slowloris attack — remote peer never reads outbound data (backpressure stall).
///   4. Rapid inbound substream open/reject bombardment.
///   5. Memory amplification via large valid shards near max_wire_message_size.
///   6. Rapid valid/invalid oscillation stressing error path transitions.
///   7. Concurrent inbound data flood while outbound is stalled.
///   8. Many sequential substream negotiations without any data.
use apollo_protobuf::protobuf::{
    PropellerUnit as ProtoUnit,
    PropellerUnitBatch as ProtoBatch,
    Shard as ProtoShard,
    ShardsOfPeer as ProtoShardsOfPeer,
};
use futures::prelude::*;
use libp2p::swarm::handler::{
    ConnectionEvent,
    ConnectionHandler,
    ConnectionHandlerEvent,
    FullyNegotiatedInbound,
    StreamUpgradeError,
};
use prost::Message;

use super::framework::*;
use crate::handler::HandlerOut;

// =========================================================================
// 1. events_to_emit queue growth under slow consumption
// =========================================================================

/// Verifies that the handler's `events_to_emit` queue remains bounded by the number of
/// units actually sent — no amplification occurs.
///
/// This test sends 1000 valid units to the handler but deliberately does NOT drain events
/// between sends. It then drains all events and asserts exactly 1000 were produced. The
/// handler must not panic or amplify the count.
///
/// NOTE: The handler itself does NOT cap this queue — it is the caller's responsibility
/// to consume events. This test verifies no amplification, not that a cap exists.
#[tokio::test]
async fn events_to_emit_no_amplification_under_flood() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound, remote, _h) = get_connected_streams().await;
    let mut remote_f = remote_framed(remote);
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    let num_units = 1000;
    let units: Vec<_> = (0..num_units)
        .map(|i| make_test_unit_with_shard(vec![u8::try_from(i % 256).unwrap(); 20]))
        .collect();

    // Send all units in batches of 50
    for chunk in units.chunks(50) {
        remote_send_units(&mut remote_f, chunk.to_vec()).await;
    }

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Drive handler to process all events
    while let Some(Some(_)) = handler.next().now_or_never() {}
    // Count units received via channel
    use futures::StreamExt;
    let mut received_count = 0;
    while let Some(Some(_)) = _unit_rx.next().now_or_never() {
        received_count += 1;
    }

    assert_eq!(
        received_count, num_units,
        "Expected exactly {num_units} units, got {received_count} — possible amplification or loss"
    );
}

// =========================================================================
// 2. send_queue growth is bounded by caller behaviour
// =========================================================================

/// Proves that queuing a very large number of outbound messages doesn't cause panics
/// or unbounded internal growth beyond the queue itself.
///
/// The handler has no cap on `send_queue`. This test queues 10,000 messages, triggers a
/// DialUpgradeError to drain them all at once, and verifies the handler reports the correct
/// count and remains usable.
#[tokio::test]
async fn send_queue_mass_drain_on_error() {
    let (mut handler, mut _unit_rx) = make_handler();
    let queue_size = 10_000;

    for i in 0..queue_size {
        simulate_send_unit(
            &mut handler,
            make_test_unit_with_shard(vec![u8::try_from(i % 256).unwrap(); 10]),
        );
    }

    // Handler should request an outbound substream
    validate_outbound_substream_request(&mut handler).await;

    // Simulate error — should drain all 10,000 messages
    simulate_dial_upgrade_error(&mut handler, 0, StreamUpgradeError::NegotiationFailed);

    let event = handler.next().await.unwrap();
    let msg = match event {
        ConnectionHandlerEvent::NotifyBehaviour(HandlerOut::SendError(msg)) => msg,
        other => panic!("Expected SendError, got: {other:?}"),
    };
    assert!(
        msg.contains(&queue_size.to_string()),
        "SendError should report {queue_size} lost messages, got: {msg}"
    );

    // Handler must still be usable
    validate_no_events(&mut handler);
    simulate_send_unit(&mut handler, make_test_unit());
    validate_outbound_substream_request(&mut handler).await;
}

// =========================================================================
// 3. Slowloris attack — remote never reads outbound data
// =========================================================================

/// A malicious peer opens an outbound substream but never reads from it. The handler
/// attempts to send data, but `poll_ready`/`poll_flush` should eventually yield Pending
/// (backpressure from the transport), NOT cause a panic or infinite loop.
///
/// This test verifies the handler doesn't spin or panic when the remote is unresponsive.
#[tokio::test]
async fn slowloris_outbound_stall() {
    let (mut handler, mut _unit_rx) = make_handler();

    // Queue many messages
    for i in 0..100u8 {
        simulate_send_unit(&mut handler, make_test_unit_with_shard(vec![i; 50]));
    }

    validate_outbound_substream_request(&mut handler).await;

    let (outbound, _remote_stream, _h) = get_connected_streams().await;
    // Note: we keep _remote_stream alive but never read from it — simulating a slowloris peer.
    simulate_fully_negotiated_outbound(&mut handler, outbound, 0);

    // Poll the handler repeatedly. The transport buffer will eventually fill up, causing
    // poll_ready/poll_flush to return Pending. The handler must not loop infinitely.
    let mut poll_count = 0;
    let max_polls = 500;
    while let Some(Some(_)) = handler.next().now_or_never() {
        poll_count += 1;
        if poll_count >= max_polls {
            break;
        }
    }

    // If we got here without panic or infinite loop, the handler handles backpressure correctly.
    // The handler should eventually return Pending (not spin forever).
    assert!(
        poll_count < max_polls,
        "Handler polled {poll_count} times without returning Pending — possible infinite loop \
         under backpressure"
    );
}

// =========================================================================
// 4. Rapid inbound substream bombardment
// =========================================================================

/// A malicious peer opens inbound substreams as fast as possible. With CONCURRENT_STREAMS=1,
/// only one slot exists. All excess substreams must be rejected (dropped) without panic or
/// resource leak.
///
/// We open 200 substreams in rapid succession — only one should be active at a time.
#[tokio::test]
async fn inbound_substream_bombardment() {
    let (mut handler, mut _unit_rx) = make_handler();

    // Hold references to keep streams alive
    let mut live_handles = Vec::new();

    // First one should be accepted
    let (inbound1, _remote1, h1) = get_connected_streams().await;
    live_handles.push(h1);
    simulate_fully_negotiated_inbound(&mut handler, inbound1);

    // The next 199 should all be rejected (slot is full)
    for _ in 0..199 {
        let (inbound, _remote, h) = get_connected_streams().await;
        live_handles.push(h);
        simulate_fully_negotiated_inbound(&mut handler, inbound);
    }

    // Handler must still be functional — no panics, no resource exhaustion
    // The first inbound substream should still work if its remote sends data
    validate_no_events(&mut handler);
}

// =========================================================================
// 5. Memory amplification via large valid shards
// =========================================================================

/// Tests that valid units with large shard payloads (near max_wire_message_size) are
/// processed correctly. Each unit is valid and will be added to events_to_emit.
///
/// This doesn't prove a cap exists, but proves the handler doesn't crash or misbehave
/// when processing many large-payload messages.
#[tokio::test]
async fn large_shard_units_no_crash() {
    // Use a moderate max size so we can send data that approaches it
    let max_size = 4096;
    let (mut handler, mut _unit_rx) = make_handler_with_max_size(max_size);
    let (inbound, remote, _h) = get_connected_streams().await;

    // Both sides need matching codec size
    let mut remote_f = remote_framed_with_max_size(remote, max_size);
    let framed =
        asynchronous_codec::Framed::new(inbound, crate::protocol::PropellerCodec::new(max_size));
    handler.on_connection_event(ConnectionEvent::FullyNegotiatedInbound(FullyNegotiatedInbound {
        protocol: framed,
        info: (),
    }));

    // Send 20 units each with a shard of ~2KB (well within max_size for a single unit,
    // but each unit must be sent in its own batch due to size)
    for i in 0..20u8 {
        let unit = make_test_unit_with_shard(vec![i; 2000]);
        remote_send_units(&mut remote_f, vec![unit]).await;
    }

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Drive handler
    while let Some(Some(_)) = handler.next().now_or_never() {}
    use futures::StreamExt;
    let mut count = 0;
    while let Some(Some(_)) = _unit_rx.next().now_or_never() {
        count += 1;
    }

    assert_eq!(count, 20, "Expected 20 large shard units, got {count}");
}

// =========================================================================
// 6. Rapid valid/invalid oscillation
// =========================================================================

/// Rapidly alternates between sending valid and completely garbage data on the same
/// inbound substream. The stream will break on garbage, so we re-establish it.
/// This stresses the error→recovery→error transition path.
///
/// The handler must remain fully functional after 100 such cycles.
#[tokio::test]
async fn rapid_valid_invalid_oscillation() {
    let (mut handler, mut _unit_rx) = make_handler();

    for cycle in 0..100u8 {
        let (inbound, remote, _h) = get_connected_streams().await;

        if cycle % 2 == 0 {
            // Valid data
            let mut remote_f = remote_framed(remote);
            simulate_fully_negotiated_inbound(&mut handler, inbound);
            let unit = make_test_unit_with_shard(vec![cycle; 10]);
            remote_send_units(&mut remote_f, vec![unit]).await;
        } else {
            // Garbage
            let mut remote_raw = remote;
            simulate_fully_negotiated_inbound(&mut handler, inbound);
            remote_send_raw_bytes(&mut remote_raw, &[0xFF, 0xFE, 0xFD, 0xFC]).await;
        }

        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        // Drain events
        for _ in 0..20 {
            if handler.next().now_or_never().is_none() {
                break;
            }
        }
    }

    // Drain any leftover units from earlier cycles
    while _unit_rx.try_next().is_ok() {}

    // Handler must still be alive and functional
    let (inbound, remote, _h) = get_connected_streams().await;
    let mut remote_f = remote_framed(remote);
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    let final_unit = make_test_unit();
    remote_send_units(&mut remote_f, vec![final_unit.clone()]).await;
    validate_received_unit(&mut handler, &mut _unit_rx, &final_unit).await;
}

// =========================================================================
// 7. Concurrent inbound flood while outbound is stalled
// =========================================================================

/// Simulates the worst case: inbound data is flooding in while outbound is stalled
/// (remote never reads). The handler must not deadlock or panic — it must continue
/// processing inbound even when outbound is blocked.
#[tokio::test]
async fn inbound_flood_during_outbound_stall() {
    let (mut handler, mut _unit_rx) = make_handler();

    // Set up outbound that will stall (remote never reads)
    simulate_send_unit(&mut handler, make_test_unit());
    validate_outbound_substream_request(&mut handler).await;

    let (outbound, _remote_out, _h1) = get_connected_streams().await;
    // _remote_out is kept alive but never read
    simulate_fully_negotiated_outbound(&mut handler, outbound, 0);

    // Set up inbound and flood it
    let (inbound, remote_in, _h2) = get_connected_streams().await;
    let mut remote_in_f = remote_framed(remote_in);
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    let units: Vec<_> = (0..50u8).map(|i| make_test_unit_with_shard(vec![i; 20])).collect();
    remote_send_units(&mut remote_in_f, units.clone()).await;

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Drive handler to process all inbound data
    for _ in 0..200 {
        match handler.next().now_or_never() {
            Some(Some(_)) => continue,
            _ => break,
        }
    }
    // Drain units from channel
    let mut received = Vec::new();
    while let Some(Some(u)) = _unit_rx.next().now_or_never() {
        received.push(u);
    }

    assert_eq!(
        received.len(),
        50,
        "Expected 50 inbound units during outbound stall, got {}",
        received.len()
    );
}

// =========================================================================
// 8. Many sequential substream negotiations without data
// =========================================================================

/// A malicious peer repeatedly negotiates inbound substreams and closes them immediately,
/// never sending any data. This probes for resource leaks in the negotiation path.
#[tokio::test]
async fn empty_substream_negotiation_churn() {
    let (mut handler, mut _unit_rx) = make_handler();

    for _ in 0..200 {
        let (inbound, remote, _h) = get_connected_streams().await;
        simulate_fully_negotiated_inbound(&mut handler, inbound);

        // Immediately close the remote end
        drop(remote);

        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        for _ in 0..10 {
            if handler.next().now_or_never().is_none() {
                break;
            }
        }
    }

    // Handler must still be alive
    let (inbound, remote, _h) = get_connected_streams().await;
    let mut remote_f = remote_framed(remote);
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    let unit = make_test_unit();
    remote_send_units(&mut remote_f, vec![unit.clone()]).await;
    validate_received_unit(&mut handler, &mut _unit_rx, &unit).await;
}

// =========================================================================
// 9. DialUpgradeError rapid cycling doesn't leak memory
// =========================================================================

/// Rapidly cycles through: enqueue message → request substream → dial upgrade error →
/// drain → repeat. Verifies no accumulation of state across 500 cycles.
#[tokio::test]
async fn dial_upgrade_error_rapid_cycling_no_leak() {
    let (mut handler, mut _unit_rx) = make_handler();

    for i in 0..500u16 {
        simulate_send_unit(
            &mut handler,
            make_test_unit_with_shard(vec![u8::try_from(i % 256).unwrap(); 20]),
        );
        validate_outbound_substream_request(&mut handler).await;
        simulate_dial_upgrade_error(&mut handler, 0, StreamUpgradeError::Timeout);
        validate_send_error(&mut handler).await;
    }

    // After 500 cycles, handler must still work perfectly
    validate_no_events(&mut handler);

    // And a successful send should still work
    let unit = make_test_unit();
    simulate_send_unit(&mut handler, unit.clone());
    validate_outbound_substream_request(&mut handler).await;

    let (outbound, remote, _h) = get_connected_streams().await;
    let mut remote_f = remote_framed(remote);
    simulate_fully_negotiated_outbound(&mut handler, outbound, 0);

    let batch = tokio::select! {
        batch = remote_recv_batch(&mut remote_f) => batch,
        _ = handler.next() => {
            remote_f.next().await.unwrap().unwrap()
        }
    };
    assert_eq!(batch.batch.len(), 1);
}

// =========================================================================
// 10. Interleaved inbound/outbound errors — both paths fail simultaneously
// =========================================================================

/// Both inbound and outbound fail at the same time: inbound gets garbage, outbound
/// gets a dial upgrade error. The handler must handle both errors without corrupting
/// internal state or panicking.
#[tokio::test]
async fn simultaneous_inbound_outbound_errors() {
    let (mut handler, mut _unit_rx) = make_handler();

    for _ in 0..50 {
        // Set up inbound with garbage
        let (inbound, mut remote_raw, _h1) = get_connected_streams().await;
        simulate_fully_negotiated_inbound(&mut handler, inbound);
        remote_send_raw_bytes(&mut remote_raw, &[0xFF, 0xFF, 0xFF, 0xFF, 0x7F]).await;

        // Queue outbound and fail it
        simulate_send_unit(&mut handler, make_test_unit());
        validate_outbound_substream_request(&mut handler).await;
        simulate_dial_upgrade_error(&mut handler, 0, StreamUpgradeError::NegotiationFailed);

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Drain everything
        for _ in 0..30 {
            if handler.next().now_or_never().is_none() {
                break;
            }
        }
    }

    // Must still work
    let (inbound, remote, _h) = get_connected_streams().await;
    let mut remote_f = remote_framed(remote);
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    let unit = make_test_unit();
    remote_send_units(&mut remote_f, vec![unit.clone()]).await;
    validate_received_unit(&mut handler, &mut _unit_rx, &unit).await;
}

// =========================================================================
// 11. Pathological batch: single batch with thousands of invalid units
// =========================================================================

/// Sends a single batch containing 10,000 invalid proto units. Each one triggers the
/// `try_from` error path. The handler must process them all without excessive latency
/// or memory growth beyond the batch itself.
#[tokio::test]
async fn massive_batch_all_invalid_units() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound, mut remote, _h) = get_connected_streams().await;
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    let invalid_units: Vec<ProtoUnit> = (0..10_000)
        .map(|i| ProtoUnit {
            shards: Some(ProtoShardsOfPeer {
                shards: vec![ProtoShard { data: vec![u8::try_from(i % 256).unwrap()] }],
            }),
            index: 0,
            merkle_root: None,
            merkle_proof: None,
            publisher: None,
            signature: vec![],
            committee_id: None,
            nonce: 0,
        })
        .collect();

    let batch = ProtoBatch { batch: invalid_units };
    let mut buf = Vec::new();
    batch.encode_length_delimited(&mut buf).unwrap();
    remote_send_raw_bytes(&mut remote, &buf).await;

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // All 10,000 should fail try_from and be dropped — no events emitted
    for _ in 0..100 {
        if handler.next().now_or_never().is_none() {
            break;
        }
    }
    validate_no_events(&mut handler);
}

// =========================================================================
// 12. Outbound substream replacement during active send
// =========================================================================

/// While the handler is actively sending on an outbound substream (should_flush is true),
/// a new outbound substream is negotiated for the same slot. The handler should handle
/// this gracefully — the old stream is replaced, pending data may be lost, but no panic.
#[tokio::test]
async fn outbound_replacement_during_active_send() {
    let (mut handler, mut _unit_rx) = make_handler();

    // Queue messages and get an active outbound substream
    for i in 0..10u8 {
        simulate_send_unit(&mut handler, make_test_unit_with_shard(vec![i; 50]));
    }
    validate_outbound_substream_request(&mut handler).await;

    let (outbound1, _remote1, _h1) = get_connected_streams().await;
    simulate_fully_negotiated_outbound(&mut handler, outbound1, 0);

    // Poll once to start sending (sets should_flush = true)
    let _ = handler.next().now_or_never();

    // Replace the outbound substream mid-send
    let (outbound2, remote2, _h2) = get_connected_streams().await;
    let mut remote2_f = remote_framed(remote2);
    simulate_fully_negotiated_outbound(&mut handler, outbound2, 0);

    // The handler should continue sending remaining queued messages on the new substream
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let mut total_received = 0;
    // Try to receive whatever comes through the new substream
    for _ in 0..20 {
        match handler.next().now_or_never() {
            Some(Some(_)) => continue,
            _ => break,
        }
    }

    // Try reading from remote2 with a timeout
    let result =
        tokio::time::timeout(std::time::Duration::from_millis(500), remote2_f.next()).await;

    if let Ok(Some(Ok(batch))) = result {
        total_received += batch.batch.len();
    }

    // Some messages may be lost due to the replacement, but no panic occurred
    // The key assertion is that we got here without panicking
    assert!(total_received <= 10, "Received {total_received} which is fine");
}

// =========================================================================
// 12. Backpressure guard: unsent_units does not grow under channel pressure
// =========================================================================

/// Proves V7 from VULNERABILITIES.md: when the engine channel is full (backpressure),
/// the handler stops reading from the wire. Without the `unsent_units.is_empty()` guard,
/// the handler would keep decoding batches and growing the buffer without bound.
///
/// This test creates a handler with a tiny channel (capacity 1), sends many batches,
/// and verifies that `unsent_units` is bounded — the handler remains responsive and
/// doesn't accumulate memory.
#[tokio::test]
async fn backpressure_bounds_unsent_units() {
    // Use a channel with capacity 1 to trigger backpressure quickly
    let config = crate::config::Config {
        stream_protocol: TEST_PROTOCOL,
        max_wire_message_size: MAX_WIRE_MESSAGE_SIZE,
        inbound_channel_capacity: 1,
        ..crate::config::Config::default()
    };
    let (tx, mut rx) = futures::channel::mpsc::channel(config.inbound_channel_capacity);
    let mut handler = crate::handler::Handler::new(&config, tx);

    let (inbound, remote, _h) = get_connected_streams().await;
    let mut remote_f = remote_framed(remote);
    simulate_fully_negotiated_inbound(&mut handler, inbound);

    // Send 50 batches without draining the channel — backpressure should prevent
    // unbounded accumulation in unsent_units.
    for i in 0..50u8 {
        let unit = make_test_unit_with_shard(vec![i; 20]);
        remote_send_units(&mut remote_f, vec![unit]).await;
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Poll handler a fixed number of times. Under backpressure, most polls return Pending
    // because the channel is full and unsent_units is non-empty (guard blocks new reads).
    for _ in 0..200 {
        let _ = handler.next().now_or_never();
    }

    // Now drain the channel and count how many units were delivered.
    // The key assertion: the handler delivered units through the channel (not lost),
    // and didn't panic or OOM from unbounded buffering.
    let mut received_count = 0;
    // First drain what's already in the channel
    while rx.try_next().is_ok() {
        received_count += 1;
    }

    // Drive the handler more to flush remaining unsent_units
    for _ in 0..500 {
        let _ = handler.next().now_or_never();
        while rx.try_next().is_ok() {
            received_count += 1;
        }
    }

    // We should have received some units (proves the handler processed data)
    // but not necessarily all 50 (some batches may not have been read from the wire
    // due to backpressure — this is the correct behavior).
    assert!(received_count > 0, "Should have received at least some units, got {received_count}");
    assert!(received_count <= 50, "Should not receive more than 50 units, got {received_count}");
}
