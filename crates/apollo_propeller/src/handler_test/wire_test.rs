use apollo_protobuf::protobuf::{PropellerUnit as ProtoUnit, PropellerUnitBatch as ProtoBatch};
use asynchronous_codec::Framed;
use futures::prelude::*;
use libp2p::swarm::handler::{ConnectionEvent, ConnectionHandler, FullyNegotiatedInbound};
use prost::Message;

use super::framework::*;
use crate::protocol::PropellerCodec;

#[tokio::test]
async fn raw_valid_length_delimited_protobuf() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound_stream, mut remote_stream, _handle) = get_connected_streams().await;

    simulate_fully_negotiated_inbound(&mut handler, inbound_stream);

    let unit = make_test_unit();
    let batch = ProtoBatch { batch: vec![ProtoUnit::from(unit.clone())] };
    let mut buf = Vec::new();
    batch.encode_length_delimited(&mut buf).unwrap();

    remote_send_raw_bytes(&mut remote_stream, &buf).await;

    validate_received_unit(&mut handler, &mut _unit_rx, &unit).await;
}

#[tokio::test]
async fn raw_truncated_varint() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound_stream, mut remote_stream, _handle) = get_connected_streams().await;

    simulate_fully_negotiated_inbound(&mut handler, inbound_stream);

    // Send a single byte that looks like the start of a multi-byte varint (high bit set)
    remote_send_raw_bytes(&mut remote_stream, &[0x80]).await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // The handler should return Pending — the codec is waiting for more varint bytes
    validate_no_events(&mut handler);
}

#[tokio::test]
async fn raw_truncated_payload() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound_stream, mut remote_stream, _handle) = get_connected_streams().await;

    simulate_fully_negotiated_inbound(&mut handler, inbound_stream);

    // Send a varint indicating 100 bytes of payload, but only send 5
    remote_send_raw_bytes(&mut remote_stream, &[0x64, 0x01, 0x02, 0x03, 0x04, 0x05]).await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    validate_no_events(&mut handler);
}

#[tokio::test]
async fn raw_oversized_message() {
    let small_max = 50;
    let (mut handler, mut _unit_rx) = make_handler_with_max_size(small_max);
    let (inbound_stream, mut remote_stream, _handle) = get_connected_streams().await;

    let framed = Framed::new(inbound_stream, PropellerCodec::new(small_max));
    handler.on_connection_event(ConnectionEvent::FullyNegotiatedInbound(FullyNegotiatedInbound {
        protocol: framed,
        info: (),
    }));

    let unit = make_test_unit_with_shard(vec![42; 200]);
    let batch = ProtoBatch { batch: vec![ProtoUnit::from(unit)] };
    let mut buf = Vec::new();
    batch.encode_length_delimited(&mut buf).unwrap();

    remote_send_raw_bytes(&mut remote_stream, &buf).await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    for _ in 0..10 {
        if handler.next().now_or_never().is_none() {
            break;
        }
    }
    validate_no_events(&mut handler);
}

#[tokio::test]
async fn raw_connection_drop_mid_message() {
    let (mut handler, mut _unit_rx) = make_handler();
    let (inbound_stream, mut remote_stream, _handle) = get_connected_streams().await;

    simulate_fully_negotiated_inbound(&mut handler, inbound_stream);

    // Send the beginning of a length-delimited message, then drop the stream
    remote_send_raw_bytes(&mut remote_stream, &[0xF4, 0x03, 0x01, 0x02]).await;

    drop(remote_stream);

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    for _ in 0..10 {
        if handler.next().now_or_never().is_none() {
            break;
        }
    }
    validate_no_events(&mut handler);
}
