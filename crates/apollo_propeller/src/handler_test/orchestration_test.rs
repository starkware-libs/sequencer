use futures::prelude::*;

use super::framework::*;

#[tokio::test]
async fn events_queue_drains_first() {
    let (mut handler, mut _unit_rx) = make_handler();

    simulate_send_unit(&mut handler, make_test_unit());
    validate_outbound_substream_request(&mut handler).await;

    let (inbound_stream, remote_inbound, _h1) = get_connected_streams().await;
    let (outbound_stream, _remote_outbound, _h2) = get_connected_streams().await;
    let mut remote_in = remote_framed(remote_inbound);

    simulate_fully_negotiated_inbound(&mut handler, inbound_stream);
    simulate_fully_negotiated_outbound(&mut handler, outbound_stream, 0);

    // Send a unit on inbound
    let inbound_unit = make_test_unit_with_shard(vec![7; 10]);
    remote_send_units(&mut remote_in, vec![inbound_unit.clone()]).await;

    // Also queue an outbound unit
    let outbound_unit = make_test_unit_with_shard(vec![8; 10]);
    simulate_send_unit(&mut handler, outbound_unit);

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Drive the handler so it processes inbound data and sends units to the channel
    for _ in 0..20 {
        match handler.next().now_or_never() {
            Some(Some(_)) => continue,
            _ => break,
        }
    }
    // We should see the inbound unit on the channel
    use futures::StreamExt;
    let received = _unit_rx.next().now_or_never();
    assert!(
        matches!(received, Some(Some(ref u)) if u == &inbound_unit),
        "Expected to receive the inbound unit"
    );
}

#[tokio::test]
async fn inbound_events_returned_in_same_poll_cycle() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound_stream, remote_stream, _handle) = get_connected_streams().await;
    let mut remote = remote_framed(remote_stream);

    simulate_fully_negotiated_inbound(&mut handler, inbound_stream);

    let unit = make_test_unit();
    remote_send_units(&mut remote, vec![unit.clone()]).await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    validate_received_unit(&mut handler, &mut _unit_rx, &unit).await;
}
