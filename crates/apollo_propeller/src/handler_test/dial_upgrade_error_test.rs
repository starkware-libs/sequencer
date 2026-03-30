use futures::prelude::*;
use libp2p::swarm::handler::{ConnectionHandlerEvent, StreamUpgradeError};

use super::framework::*;
use crate::handler::HandlerOut;

#[tokio::test]
async fn dial_upgrade_timeout_drains_queue() {
    let (mut handler, _unit_rx) = make_handler();
    let unit = make_test_unit();

    simulate_send_unit(&mut handler, unit);
    validate_outbound_substream_request(&mut handler).await;

    simulate_dial_upgrade_error(&mut handler, 0, StreamUpgradeError::Timeout);

    // The handler should drain the queue and emit a SendError (not re-request a substream)
    validate_send_error(&mut handler).await;
    validate_no_events(&mut handler);
}

#[tokio::test]
async fn dial_upgrade_negotiation_failed_drains_queue() {
    let (mut handler, _unit_rx) = make_handler();
    let unit = make_test_unit();

    simulate_send_unit(&mut handler, unit);
    validate_outbound_substream_request(&mut handler).await;

    simulate_dial_upgrade_error(&mut handler, 0, StreamUpgradeError::NegotiationFailed);

    validate_send_error(&mut handler).await;
    validate_no_events(&mut handler);
}

#[tokio::test]
async fn dial_upgrade_io_error_drains_queue() {
    let (mut handler, _unit_rx) = make_handler();
    let unit = make_test_unit();

    simulate_send_unit(&mut handler, unit);
    validate_outbound_substream_request(&mut handler).await;

    simulate_dial_upgrade_error(
        &mut handler,
        0,
        StreamUpgradeError::Io(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "test io error",
        )),
    );

    validate_send_error(&mut handler).await;
    validate_no_events(&mut handler);
}

#[tokio::test]
async fn dial_upgrade_error_reports_dropped_count() {
    let (mut handler, _unit_rx) = make_handler();

    // Queue 5 messages
    for _ in 0..5 {
        simulate_send_unit(&mut handler, make_test_unit());
    }
    validate_outbound_substream_request(&mut handler).await;

    simulate_dial_upgrade_error(&mut handler, 0, StreamUpgradeError::NegotiationFailed);

    // The SendError should mention the count of dropped messages
    let event = handler.next().await.unwrap();
    let msg = match event {
        ConnectionHandlerEvent::NotifyBehaviour(HandlerOut::SendError(msg)) => msg,
        other => panic!("Expected SendError, got: {other:?}"),
    };
    assert!(
        msg.contains("5") && msg.contains("lost"),
        "SendError should report 5 lost messages, got: {msg}"
    );
}

#[tokio::test]
async fn dial_upgrade_error_when_not_pending() {
    let (mut handler, _unit_rx) = make_handler();

    // Outbound state is Idle, not Pending — should log error but not crash
    simulate_dial_upgrade_error(&mut handler, 0, StreamUpgradeError::Timeout);

    validate_no_events(&mut handler);
}

#[tokio::test]
async fn dial_upgrade_error_with_empty_queue() {
    let (mut handler, _unit_rx) = make_handler();
    let unit = make_test_unit();
    simulate_send_unit(&mut handler, unit);
    validate_outbound_substream_request(&mut handler).await;

    // Negotiate outbound and let the handler send the queued unit
    let (outbound_stream, remote_stream, _handle) = get_connected_streams().await;
    let mut remote = remote_framed(remote_stream);
    simulate_fully_negotiated_outbound(&mut handler, outbound_stream, 0);

    // Drive the handler to send and consume the message from remote
    let _batch = tokio::select! {
        batch = remote_recv_batch(&mut remote) => batch,
        _ = handler.next() => {
            remote.next().await.unwrap().unwrap()
        }
    };

    // Queue is now empty — no new outbound request or SendError should be emitted
    validate_no_events(&mut handler);
}

#[tokio::test]
async fn no_renegotiation_loop_on_empty_queue() {
    let (mut handler, _unit_rx) = make_handler();
    let unit = make_test_unit();
    simulate_send_unit(&mut handler, unit);
    validate_outbound_substream_request(&mut handler).await;

    // Negotiate outbound and let the handler send the queued unit
    let (outbound_stream, remote_stream, _handle) = get_connected_streams().await;
    let mut remote = remote_framed(remote_stream);
    simulate_fully_negotiated_outbound(&mut handler, outbound_stream, 0);

    // Drive the handler to send and consume the message from remote
    let _batch = tokio::select! {
        batch = remote_recv_batch(&mut remote) => batch,
        _ = handler.next() => {
            remote.next().await.unwrap().unwrap()
        }
    };

    // Queue is now empty — no new outbound request should be made
    validate_no_events(&mut handler);
}

#[tokio::test]
async fn handler_usable_after_dial_upgrade_error() {
    let (mut handler, _unit_rx) = make_handler();

    // First attempt: queue a unit, get error, queue is drained
    simulate_send_unit(&mut handler, make_test_unit());
    validate_outbound_substream_request(&mut handler).await;
    simulate_dial_upgrade_error(&mut handler, 0, StreamUpgradeError::Timeout);
    validate_send_error(&mut handler).await;

    // Second attempt: queue a new unit — handler should request a fresh substream
    let unit = make_test_unit();
    simulate_send_unit(&mut handler, unit.clone());
    validate_outbound_substream_request(&mut handler).await;

    // This time negotiation succeeds
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
