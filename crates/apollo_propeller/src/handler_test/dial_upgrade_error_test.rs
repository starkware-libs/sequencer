use futures::prelude::*;
use libp2p::swarm::handler::StreamUpgradeError;

use super::framework::*;

#[tokio::test]
async fn dial_upgrade_timeout() {
    let mut handler = make_handler();
    let unit = make_test_unit();

    simulate_send_unit(&mut handler, unit);
    validate_outbound_substream_request(&mut handler).await;

    simulate_dial_upgrade_error(&mut handler, 0, StreamUpgradeError::Timeout);

    // Queue is non-empty, so a new substream request should be emitted
    validate_outbound_substream_request(&mut handler).await;
}

#[tokio::test]
async fn dial_upgrade_negotiation_failed() {
    let mut handler = make_handler();
    let unit = make_test_unit();

    simulate_send_unit(&mut handler, unit);
    validate_outbound_substream_request(&mut handler).await;

    simulate_dial_upgrade_error(&mut handler, 0, StreamUpgradeError::NegotiationFailed);

    validate_outbound_substream_request(&mut handler).await;
}

#[tokio::test]
async fn dial_upgrade_io_error() {
    let mut handler = make_handler();
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

    validate_outbound_substream_request(&mut handler).await;
}

#[tokio::test]
async fn dial_upgrade_error_when_not_pending() {
    let mut handler = make_handler();

    // Outbound state is Idle, not Pending — should log error but not crash
    simulate_dial_upgrade_error(&mut handler, 0, StreamUpgradeError::Timeout);

    validate_no_events(&mut handler);
}

#[tokio::test]
async fn no_renegotiation_loop_on_empty_queue() {
    let mut handler = make_handler();
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
async fn renegotiation_on_nonempty_queue() {
    let mut handler = make_handler();
    let unit = make_test_unit();

    simulate_send_unit(&mut handler, unit);
    validate_outbound_substream_request(&mut handler).await;

    simulate_dial_upgrade_error(&mut handler, 0, StreamUpgradeError::Timeout);

    // Queue is non-empty, so handler should request a new substream
    validate_outbound_substream_request(&mut handler).await;
}
