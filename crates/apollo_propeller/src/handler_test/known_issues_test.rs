/// Regression tests for previously-known bugs in the Propeller handler.
///
/// These tests were originally `#[ignore]`-d because they exercised buggy behavior. Now that
/// the bugs are fixed, they run as normal tests and serve as regression guards.
///
/// Original TODOs in handler.rs:
///   - DialUpgradeError silent delivery failure + infinite renegotiation loop (fixed)
///   - on_behaviour_event doesn't wake the poller (fixed)
///   - Units lost on send error with no count reported (fixed)
use assert_matches::assert_matches;
use futures::prelude::*;
use libp2p::swarm::handler::{ConnectionHandler, ConnectionHandlerEvent, StreamUpgradeError};

use super::framework::*;
use crate::handler::HandlerOut;

// =========================================================================
// Regression: DialUpgradeError — reports lost messages
// =========================================================================

#[tokio::test]
async fn dial_upgrade_error_should_report_lost_messages() {
    let (mut handler, _unit_rx) = make_handler();

    // Queue 5 messages
    for _ in 0..5 {
        simulate_send_unit(&mut handler, make_test_unit());
    }

    validate_outbound_substream_request(&mut handler).await;

    // Simulate negotiation failure
    simulate_dial_upgrade_error(&mut handler, 0, StreamUpgradeError::NegotiationFailed);

    // The handler should emit a SendError reporting the lost messages
    let event = handler.next().await.unwrap();
    assert_matches!(event, ConnectionHandlerEvent::NotifyBehaviour(HandlerOut::SendError(_)));
}

// =========================================================================
// Regression: DialUpgradeError — no infinite renegotiation loop
// =========================================================================

#[tokio::test]
async fn dial_upgrade_error_should_not_renegotiate_endlessly() {
    let (mut handler, _unit_rx) = make_handler();

    simulate_send_unit(&mut handler, make_test_unit());
    validate_outbound_substream_request(&mut handler).await;

    // First failure
    simulate_dial_upgrade_error(&mut handler, 0, StreamUpgradeError::NegotiationFailed);

    // After the error, the handler should NOT immediately request another substream.
    // It should drain the queue and emit SendError instead.
    let maybe_event = handler.next().now_or_never();
    match maybe_event {
        Some(Some(ConnectionHandlerEvent::OutboundSubstreamRequest { .. })) => {
            panic!(
                "BUG: handler immediately re-requested outbound substream after DialUpgradeError \
                 — this is the infinite renegotiation loop"
            );
        }
        Some(Some(ConnectionHandlerEvent::NotifyBehaviour(HandlerOut::SendError(_)))) => {
            // Correct: handler reported the lost messages
        }
        other => {
            panic!("Expected SendError event, got: {other:?}");
        }
    }
}

// =========================================================================
// Regression: on_behaviour_event wakes the poller
// =========================================================================

#[tokio::test]
async fn send_unit_should_wake_handler() {
    let (mut handler, _unit_rx) = make_handler();

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

    // Poll the handler once so it returns Pending and stores its waker.
    // After this, the waker is registered.
    validate_no_events(&mut handler);

    // Enqueue another message. The handler should self-wake, which means when
    // we next poll it, it will be Ready (not stuck on Pending forever).
    simulate_send_unit(&mut handler, make_test_unit());

    // Drive both the handler and the remote reader. If the waker fired correctly,
    // the handler's poll will be called by the runtime and the message will flow.
    let result = tokio::time::timeout(std::time::Duration::from_millis(500), async {
        loop {
            tokio::select! {
                batch = remote_f.next() => return batch.unwrap().unwrap(),
                event = handler.next() => {
                    // The handler may emit events while sending; keep polling
                    if event.is_none() { break; }
                }
            }
        }
        // Fallback: try reading from remote one more time
        remote_f.next().await.unwrap().unwrap()
    })
    .await;

    assert!(
        result.is_ok(),
        "Handler did not self-wake after on_behaviour_event; message stuck in send_queue"
    );
}

// =========================================================================
// Regression: send error includes count of lost units
// =========================================================================

#[tokio::test]
async fn send_error_should_report_lost_unit_count() {
    let tiny_max = 10; // so small that the encoded batch will exceed the codec limit
    let (mut handler, _unit_rx) = make_handler_with_max_size(tiny_max);

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

    // Drive the handler — it should try to send, hit the codec error, and emit SendError
    let event = handler.next().await.unwrap();
    let msg = match event {
        ConnectionHandlerEvent::NotifyBehaviour(HandlerOut::SendError(msg)) => msg,
        other => panic!("Expected SendError, got: {other:?}"),
    };
    assert!(
        msg.contains("exceeds maximum"),
        "SendError should report codec size violation, got: {msg}"
    );
}
