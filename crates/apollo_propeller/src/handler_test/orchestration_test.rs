use futures::prelude::*;
use libp2p::swarm::handler::ConnectionHandlerEvent;

use super::framework::*;
use crate::handler::HandlerOut;

#[tokio::test]
async fn events_queue_drains_first() {
    let mut handler = make_handler();

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

    // We should eventually see the inbound unit
    let mut saw_inbound = false;
    for _ in 0..20 {
        match handler.next().now_or_never() {
            Some(Some(ConnectionHandlerEvent::NotifyBehaviour(HandlerOut::Unit(ref u))))
                if u == &inbound_unit =>
            {
                saw_inbound = true;
                break;
            }
            Some(Some(_)) => continue,
            _ => break,
        }
    }
    assert!(saw_inbound, "Expected to receive the inbound unit");
}

#[tokio::test]
async fn inbound_events_returned_in_same_poll_cycle() {
    let mut handler = make_handler();
    let (inbound_stream, remote_stream, _handle) = get_connected_streams().await;
    let mut remote = remote_framed(remote_stream);

    simulate_fully_negotiated_inbound(&mut handler, inbound_stream);

    let unit = make_test_unit();
    remote_send_units(&mut remote, vec![unit.clone()]).await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    validate_received_unit(&mut handler, &unit).await;
}
