use apollo_protobuf::protobuf::{
    PropellerUnit as ProtoUnit,
    PropellerUnitBatch as ProtoBatch,
    Shard as ProtoShard,
    ShardsOfPeer as ProtoShardsOfPeer,
};
use futures::prelude::*;

use super::framework::*;
use crate::PropellerUnit;

#[tokio::test]
async fn receive_single_unit() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound_stream, remote_stream, _handle) = get_connected_streams().await;
    let mut remote = remote_framed(remote_stream);

    simulate_fully_negotiated_inbound(&mut handler, inbound_stream);

    let unit = make_test_unit();
    remote_send_units(&mut remote, vec![unit.clone()]).await;

    validate_received_unit(&mut handler, &mut _unit_rx, &unit).await;
    validate_no_events(&mut handler);
}

#[tokio::test]
async fn receive_batch_of_units() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound_stream, remote_stream, _handle) = get_connected_streams().await;
    let mut remote = remote_framed(remote_stream);

    simulate_fully_negotiated_inbound(&mut handler, inbound_stream);

    let units: Vec<PropellerUnit> =
        (0..5).map(|i| make_test_unit_with_shard(vec![i; 10])).collect();
    remote_send_units(&mut remote, units.clone()).await;

    for unit in &units {
        validate_received_unit(&mut handler, &mut _unit_rx, unit).await;
    }
    validate_no_events(&mut handler);
}

#[tokio::test]
async fn receive_multiple_sequential_batches() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound_stream, remote_stream, _handle) = get_connected_streams().await;
    let mut remote = remote_framed(remote_stream);

    simulate_fully_negotiated_inbound(&mut handler, inbound_stream);

    let unit1 = make_test_unit_with_shard(vec![1]);
    remote_send_units(&mut remote, vec![unit1.clone()]).await;
    validate_received_unit(&mut handler, &mut _unit_rx, &unit1).await;

    let unit2 = make_test_unit_with_shard(vec![2]);
    remote_send_units(&mut remote, vec![unit2.clone()]).await;
    validate_received_unit(&mut handler, &mut _unit_rx, &unit2).await;

    validate_no_events(&mut handler);
}

#[tokio::test]
async fn inbound_stream_closed_by_remote() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound_stream, remote_stream, _handle) = get_connected_streams().await;
    let mut remote = remote_framed(remote_stream);

    simulate_fully_negotiated_inbound(&mut handler, inbound_stream);

    // Close the remote end
    remote.close().await.unwrap();

    // Poll the handler — the inbound substream should transition to Closing, then be freed.
    for _ in 0..5 {
        if handler.next().now_or_never().is_none() {
            break;
        }
    }
    validate_no_events(&mut handler);
}

#[tokio::test]
async fn inbound_stream_read_error() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound_stream, mut remote_stream, _handle) = get_connected_streams().await;

    simulate_fully_negotiated_inbound(&mut handler, inbound_stream);

    // Send garbage bytes — this will cause a codec decode error
    remote_send_raw_bytes(&mut remote_stream, &[0xFF, 0xFF, 0xFF, 0xFF, 0x7F]).await;

    // Poll until the handler processes the error and transitions to Closing.
    for _ in 0..10 {
        if handler.next().now_or_never().is_none() {
            break;
        }
    }
    validate_no_events(&mut handler);
}

#[tokio::test]
async fn inbound_slot_full_rejects_new_substream() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound_stream1, _remote1, _handle1) = get_connected_streams().await;
    let (inbound_stream2, _remote2, _handle2) = get_connected_streams().await;

    // Fill the only slot
    simulate_fully_negotiated_inbound(&mut handler, inbound_stream1);

    // Attempt to add a second inbound substream — should be rejected (dropped)
    simulate_fully_negotiated_inbound(&mut handler, inbound_stream2);

    // Handler should still work with the first substream
    validate_no_events(&mut handler);
}

#[tokio::test]
async fn invalid_protobuf_unit_in_batch() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound_stream, remote_stream, _handle) = get_connected_streams().await;
    let mut remote = remote_framed(remote_stream);

    simulate_fully_negotiated_inbound(&mut handler, inbound_stream);

    // Send a batch with an invalid unit (missing required fields) alongside a valid one
    let valid_unit = make_test_unit();
    let invalid_proto_unit = ProtoUnit {
        shards: Some(ProtoShardsOfPeer { shards: vec![ProtoShard { data: vec![1, 2, 3] }] }),
        index: 0,
        merkle_root: None,  // Missing required field
        merkle_proof: None, // Missing required field
        publisher: None,    // Missing required field
        signature: vec![],
        committee_id: None,
        nonce: 0,
    };
    let batch = ProtoBatch { batch: vec![ProtoUnit::from(valid_unit.clone()), invalid_proto_unit] };
    remote.send(batch).await.unwrap();

    // The valid unit should be received; the invalid one should be silently warned
    validate_received_unit(&mut handler, &mut _unit_rx, &valid_unit).await;
    validate_no_events(&mut handler);
}

#[tokio::test]
async fn inbound_closing_error_frees_slot() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound_stream, remote_stream, _handle) = get_connected_streams().await;

    simulate_fully_negotiated_inbound(&mut handler, inbound_stream);

    // Drop the remote end abruptly (without closing gracefully)
    drop(remote_stream);

    // Give the handler time to process the close
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Poll several times to let the inbound substream complete its Closing transition
    for _ in 0..10 {
        if handler.next().now_or_never().is_none() {
            break;
        }
    }

    // The slot should be freed — we should be able to add a new inbound substream
    let (inbound_stream2, remote_stream2, _handle2) = get_connected_streams().await;
    let mut remote2 = remote_framed(remote_stream2);
    simulate_fully_negotiated_inbound(&mut handler, inbound_stream2);

    let unit = make_test_unit();
    remote_send_units(&mut remote2, vec![unit.clone()]).await;
    validate_received_unit(&mut handler, &mut _unit_rx, &unit).await;
}
