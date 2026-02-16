/// Tests that document known bugs in the Propeller handler.
///
/// Each test in this file exercises a behavior that the TODOs in `handler.rs` identify as
/// incorrect. The tests assert what *should* happen (the correct behavior). Today they will
/// fail because the bugs are still present. When a bug is fixed, the corresponding test will
/// start passing — at that point the `#[ignore]` annotation should be removed.
///
/// Referencing TODOs by line range in handler.rs at time of writing:
///   - Lines 546-553: DialUpgradeError silent delivery failure + infinite renegotiation loop
///   - Line 476: on_behaviour_event doesn't wake the poller
///   - Lines 389, 398: Units lost on send error with no retry or count
use assert_matches::assert_matches;
use futures::prelude::*;
use libp2p::swarm::handler::{ConnectionHandler, ConnectionHandlerEvent, StreamUpgradeError};

use super::framework::*;
use crate::handler::HandlerOut;

// =========================================================================
// TODO(handler.rs:546-553): DialUpgradeError — silent delivery failure
//
// Current behavior: After a DialUpgradeError, the send_queue retains messages
// but no SendError is emitted to the behaviour. The caller has no way to know
// their messages are stuck in a dead-end queue.
//
// Correct behavior: The handler should drain the send_queue and emit a
// SendError to the behaviour reporting the number of dropped messages.
// =========================================================================

#[tokio::test]
#[ignore = "known bug: DialUpgradeError does not emit SendError for queued messages \
            (handler.rs:546-553)"]
async fn dial_upgrade_error_should_report_lost_messages() {
    let mut handler = make_handler();

    // Queue 5 messages
    for _ in 0..5 {
        simulate_send_unit(&mut handler, make_test_unit());
    }

    // Trigger substream request
    validate_outbound_substream_request(&mut handler).await;

    // Simulate negotiation failure — messages can never be delivered on this substream
    simulate_dial_upgrade_error(&mut handler, 0, StreamUpgradeError::NegotiationFailed);

    // CORRECT behavior: the handler should emit a SendError telling the behaviour that
    // messages were lost, rather than silently keeping them in the queue.
    let event = handler.next().await.unwrap();
    assert_matches!(event, ConnectionHandlerEvent::NotifyBehaviour(HandlerOut::SendError(_)));
}

// =========================================================================
// TODO(handler.rs:546-553): DialUpgradeError — infinite renegotiation loop
//
// Current behavior: After a DialUpgradeError with a non-empty queue, the
// handler resets to Idle and immediately re-requests a substream on the next
// poll. If the remote peer doesn't support the protocol, this loops forever:
//   DialUpgradeError → Idle → OutboundSubstreamRequest → DialUpgradeError → ...
//
// Correct behavior: The handler should either:
//   (a) drain the queue and report failure, or
//   (b) implement exponential backoff before retrying.
// The simplest correct fix from the TODO is (a).
// =========================================================================

#[tokio::test]
#[ignore = "known bug: infinite renegotiation loop against unsupported peer (handler.rs:546-553)"]
async fn dial_upgrade_error_should_not_renegotiate_endlessly() {
    let mut handler = make_handler();

    simulate_send_unit(&mut handler, make_test_unit());
    validate_outbound_substream_request(&mut handler).await;

    // First failure
    simulate_dial_upgrade_error(&mut handler, 0, StreamUpgradeError::NegotiationFailed);

    // CORRECT behavior: after the error, the handler should NOT immediately request another
    // substream. It should drain the queue and emit SendError(s) instead.
    //
    // TODAY this fails because the handler DOES request another substream (the renegotiation
    // loop bug). We assert that the handler does NOT produce an OutboundSubstreamRequest.
    let maybe_event = handler.next().now_or_never();
    match maybe_event {
        Some(Some(ConnectionHandlerEvent::OutboundSubstreamRequest { .. })) => {
            panic!(
                "BUG: handler immediately re-requested outbound substream after DialUpgradeError \
                 — this is the infinite renegotiation loop"
            );
        }
        _ => {
            // Good: handler did not immediately re-request
        }
    }
}

// =========================================================================
// TODO(handler.rs:476): on_behaviour_event doesn't wake the poller
//
// Current behavior: on_behaviour_event(SendUnit) pushes to send_queue but
// does not call cx.waker().wake_by_ref(). The message sits in the queue
// until something else causes poll to be called.
//
// Correct behavior: After enqueueing, the handler should wake the waker so
// the runtime will call poll() promptly.
//
// Note: This is hard to test in isolation without access to the waker, but
// we can demonstrate the symptom: if nothing else is driving the handler,
// a newly-enqueued message won't trigger any event until an external poll.
// =========================================================================

#[tokio::test]
#[ignore = "known bug: on_behaviour_event doesn't wake poller (handler.rs:476)"]
async fn send_unit_should_wake_handler() {
    let mut handler = make_handler();

    // Set up an active outbound substream so messages can be sent immediately
    let (outbound, remote, _h) = get_connected_streams().await;
    simulate_send_unit(&mut handler, make_test_unit());
    validate_outbound_substream_request(&mut handler).await;
    simulate_fully_negotiated_outbound(&mut handler, outbound, 0);

    // Drive the handler to send the first unit and flush
    let mut remote_f = remote_framed(remote);
    let _batch = tokio::select! {
        batch = remote_recv_batch(&mut remote_f) => batch,
        _ = handler.next() => {
            remote_f.next().await.unwrap().unwrap()
        }
    };

    // Now enqueue another message. Because the handler doesn't wake itself,
    // we'd need to externally poll it. In a correct implementation, the handler
    // should self-wake so that `tokio::select!` on handler.next() resolves promptly.
    simulate_send_unit(&mut handler, make_test_unit());

    // Give the handler a generous deadline — if it self-wakes, the message should
    // arrive at the remote within the timeout.
    let result = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        remote_recv_batch(&mut remote_f),
    )
    .await;

    assert!(
        result.is_ok(),
        "BUG: handler did not self-wake after on_behaviour_event; message is stuck in send_queue \
         until an external event triggers poll"
    );
}

// =========================================================================
// TODO(handler.rs:389,398): Units lost on send error — no retry, no count
//
// Current behavior: When start_send or poll_ready errors, the batch that was
// popped from send_queue is silently lost. The SendError event contains only
// the IO error message, not the number of lost units or their content.
//
// Correct behavior: The handler should either:
//   (a) re-enqueue the batch for retry, or
//   (b) report how many units were lost in the SendError.
// =========================================================================

#[tokio::test]
#[ignore = "known bug: units lost on send error are not re-enqueued or counted (handler.rs:389,398)"]
async fn send_error_should_not_silently_lose_units() {
    // This test verifies that after a send error, the handler either retries
    // the lost batch or reports the count. Currently it does neither.
    //
    // Triggering a real start_send error is non-trivial in the test harness
    // (we'd need to break the underlying stream at the right moment), so this
    // test documents the issue by exercising the batch-then-drop path through
    // the oversized-message codepath.
    let tiny_max = 10; // so small that the encoded batch will exceed the codec limit
    let mut handler = make_handler_with_max_size(tiny_max);

    // Queue a unit whose encoded size exceeds tiny_max
    let big_unit = make_test_unit_with_shard(vec![42; 200]);
    simulate_send_unit(&mut handler, big_unit);

    validate_outbound_substream_request(&mut handler).await;

    let (outbound, _remote, _h) = get_connected_streams().await;
    // Use a codec with the same tiny limit so encoding actually fails
    let framed =
        asynchronous_codec::Framed::new(outbound, crate::protocol::PropellerCodec::new(tiny_max));
    handler.on_connection_event(libp2p::swarm::handler::ConnectionEvent::FullyNegotiatedOutbound(
        libp2p::swarm::handler::FullyNegotiatedOutbound { protocol: framed, info: 0 },
    ));

    // Drive the handler — it should try to send, hit the codec error, and emit SendError.
    let event = handler.next().await.unwrap();
    assert_matches!(
        event,
        ConnectionHandlerEvent::NotifyBehaviour(HandlerOut::SendError(ref _msg))
    );

    // NOW — the unit that was popped from the queue is gone forever.
    // CORRECT behavior: the handler should either:
    //   1. Re-enqueue the lost units so a subsequent send attempt can retry, OR
    //   2. Include the count of lost units in the SendError so the behaviour knows.
    //
    // Since we can't introspect send_queue from here, and the current SendError
    // is just a string, we assert that the error message includes the count.
    // This will fail because the current implementation only includes the IO error.
    let msg = match event {
        ConnectionHandlerEvent::NotifyBehaviour(HandlerOut::SendError(msg)) => msg,
        _ => unreachable!(),
    };
    assert!(
        msg.contains("1 unit") || msg.contains("lost"),
        "BUG: SendError should report lost unit count, got: {msg}"
    );
}
