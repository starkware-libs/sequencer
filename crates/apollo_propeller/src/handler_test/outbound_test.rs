use futures::prelude::*;

use super::framework::*;
use crate::PropellerUnit;

#[tokio::test]
async fn send_single_unit() {
    let (mut handler, _unit_rx) = make_handler();
    let unit = make_test_unit();

    simulate_send_unit(&mut handler, unit.clone());
    validate_outbound_substream_request(&mut handler).await;

    let (outbound_stream, remote_stream, _handle) = get_connected_streams().await;
    let mut remote = remote_framed(remote_stream);
    simulate_fully_negotiated_outbound(&mut handler, outbound_stream, 0);

    let recv_future = remote_recv_batch(&mut remote);
    let mut fused_handler = handler.fuse();

    let batch = tokio::select! {
        batch = recv_future => batch,
        _ = fused_handler.next() => {
            remote.next().await.unwrap().unwrap()
        }
    };

    assert_eq!(batch.batch.len(), 1);
    let received_unit = PropellerUnit::try_from(batch.batch.into_iter().next().unwrap()).unwrap();
    assert_eq!(received_unit, unit);
}

#[tokio::test]
async fn send_batch_of_units() {
    let (mut handler, _unit_rx) = make_handler();

    let units: Vec<PropellerUnit> =
        (0..3).map(|i| make_test_unit_with_shard(vec![i; 10])).collect();

    for unit in &units {
        simulate_send_unit(&mut handler, unit.clone());
    }

    validate_outbound_substream_request(&mut handler).await;

    let (outbound_stream, remote_stream, _handle) = get_connected_streams().await;
    let mut remote = remote_framed(remote_stream);
    simulate_fully_negotiated_outbound(&mut handler, outbound_stream, 0);

    let batch = tokio::select! {
        batch = remote_recv_batch(&mut remote) => batch,
        _ = handler.next() => {
            remote.next().await.unwrap().unwrap()
        }
    };

    assert_eq!(batch.batch.len(), 3);
    for (proto_unit, expected) in batch.batch.into_iter().zip(units.iter()) {
        let received = PropellerUnit::try_from(proto_unit).unwrap();
        assert_eq!(&received, expected);
    }
}

#[tokio::test]
async fn batch_respects_max_wire_size() {
    let small_max = 200;
    let (mut handler, _unit_rx) = make_handler_with_max_size(small_max);

    let unit1 = make_test_unit_with_shard(vec![1; 50]);
    let unit2 = make_test_unit_with_shard(vec![2; 50]);

    simulate_send_unit(&mut handler, unit1.clone());
    simulate_send_unit(&mut handler, unit2.clone());

    validate_outbound_substream_request(&mut handler).await;

    let (outbound_stream, remote_stream, _handle) = get_connected_streams().await;
    // Remote uses a large max so it can read batches even if they exceed the handler's limit
    let mut remote = remote_framed(remote_stream);
    simulate_fully_negotiated_outbound(&mut handler, outbound_stream, 0);

    let batch1 = tokio::select! {
        batch = remote_recv_batch(&mut remote) => batch,
        _ = handler.next() => {
            remote.next().await.unwrap().unwrap()
        }
    };

    assert!(!batch1.batch.is_empty());

    let total_received = if batch1.batch.len() < 2 {
        let batch2 = tokio::select! {
            batch = remote_recv_batch(&mut remote) => batch,
            _ = handler.next() => {
                remote.next().await.unwrap().unwrap()
            }
        };
        batch1.batch.len() + batch2.batch.len()
    } else {
        batch1.batch.len()
    };

    assert_eq!(total_received, 2);
}

#[tokio::test]
async fn idle_with_empty_queue_no_substream_request() {
    let (mut handler, _unit_rx) = make_handler();
    validate_no_events(&mut handler);
}

#[tokio::test]
async fn pending_state_waits_for_negotiation() {
    let (mut handler, _unit_rx) = make_handler();
    let unit = make_test_unit();

    simulate_send_unit(&mut handler, unit);
    validate_outbound_substream_request(&mut handler).await;

    // Substream is Pending, no negotiation yet — should return Pending
    validate_no_events(&mut handler);
}

#[tokio::test]
async fn outbound_substream_replaced_with_pending_data() {
    let (mut handler, _unit_rx) = make_handler();
    let unit = make_test_unit();

    simulate_send_unit(&mut handler, unit);
    validate_outbound_substream_request(&mut handler).await;

    let (outbound_stream1, _remote1, _handle1) = get_connected_streams().await;
    simulate_fully_negotiated_outbound(&mut handler, outbound_stream1, 0);

    // Send a unit so should_flush becomes true
    let unit2 = make_test_unit_with_shard(vec![9; 10]);
    simulate_send_unit(&mut handler, unit2);

    // Poll to trigger the send (setting should_flush = true)
    let _ = handler.next().now_or_never();

    // Negotiate a new outbound substream for the same index (replacing with pending data)
    let (outbound_stream2, _remote2, _handle2) = get_connected_streams().await;
    simulate_fully_negotiated_outbound(&mut handler, outbound_stream2, 0);

    validate_no_events(&mut handler);
}

#[tokio::test]
async fn flush_after_send() {
    let (mut handler, _unit_rx) = make_handler();
    let unit = make_test_unit();

    simulate_send_unit(&mut handler, unit.clone());
    validate_outbound_substream_request(&mut handler).await;

    let (outbound_stream, remote_stream, _handle) = get_connected_streams().await;
    let mut remote = remote_framed(remote_stream);
    simulate_fully_negotiated_outbound(&mut handler, outbound_stream, 0);

    let batch = tokio::select! {
        batch = remote_recv_batch(&mut remote) => batch,
        _ = handler.next() => {
            remote.next().await.unwrap().unwrap()
        }
    };

    assert_eq!(batch.batch.len(), 1);
    let received = PropellerUnit::try_from(batch.batch.into_iter().next().unwrap()).unwrap();
    assert_eq!(received, unit);
}
