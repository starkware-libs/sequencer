use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context, Poll};

use apollo_protobuf::protobuf::{PropellerUnit as ProtoUnit, PropellerUnitBatch as ProtoBatch};
use assert_matches::assert_matches;
use asynchronous_codec::Framed;
use futures::prelude::*;
use libp2p::swarm::handler::{
    ConnectionEvent,
    ConnectionHandler,
    ConnectionHandlerEvent,
    DialUpgradeError,
    FullyNegotiatedInbound,
    FullyNegotiatedOutbound,
    StreamUpgradeError,
};
use libp2p::swarm::{Stream, StreamProtocol, Swarm, SwarmEvent};
use libp2p::PeerId;
use libp2p_swarm_test::SwarmExt;
use tokio::task::JoinHandle;

use super::get_stream;
use crate::config::Config;
use crate::handler::{Handler, HandlerIn, HandlerOut};
use crate::protocol::PropellerCodec;
use crate::types::{CommitteeId, MessageRoot, ShardIndex};
use crate::unit::{Shard, ShardsOfPeer};
use crate::{MerkleProof, PropellerUnit};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const TEST_PROTOCOL: StreamProtocol = StreamProtocol::new("/propeller/test/1");
pub const MAX_WIRE_MESSAGE_SIZE: usize = 1_048_576; // 1 MiB

// ---------------------------------------------------------------------------
// Type alias & Stream impl for polling the handler
// ---------------------------------------------------------------------------

pub type HandlerEvent = ConnectionHandlerEvent<
    <Handler as ConnectionHandler>::OutboundProtocol,
    <Handler as ConnectionHandler>::OutboundOpenInfo,
    <Handler as ConnectionHandler>::ToBehaviour,
>;

impl Unpin for Handler {}

impl futures::Stream for Handler {
    type Item = HandlerEvent;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match ConnectionHandler::poll(Pin::into_inner(self), cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(event) => Poll::Ready(Some(event)),
        }
    }
}

// ---------------------------------------------------------------------------
// Connected streams
// ---------------------------------------------------------------------------

/// Create two streams that are connected to each other. Return them and a join handle for a task
/// that will drive the swarm event loop (this task will run forever so it shouldn't be awaited).
pub async fn get_connected_streams() -> (Stream, Stream, JoinHandle<()>) {
    let mut swarm1 = Swarm::new_ephemeral_tokio(|_| get_stream::Behaviour::default());
    let mut swarm2 = Swarm::new_ephemeral_tokio(|_| get_stream::Behaviour::default());
    swarm1.listen().with_memory_addr_external().await;
    swarm2.listen().with_memory_addr_external().await;

    swarm1.connect(&mut swarm2).await;

    let merged_swarm = tokio_stream::StreamExt::merge(swarm1, swarm2);
    let mut filtered_swarm = tokio_stream::StreamExt::filter_map(merged_swarm, |event| {
        if let SwarmEvent::Behaviour(stream) = event { Some(stream) } else { None }
    });
    (
        tokio_stream::StreamExt::next(&mut filtered_swarm).await.unwrap(),
        tokio_stream::StreamExt::next(&mut filtered_swarm).await.unwrap(),
        tokio::task::spawn(async move {
            while tokio_stream::StreamExt::next(&mut filtered_swarm).await.is_some() {}
        }),
    )
}

// ---------------------------------------------------------------------------
// Test unit factories
// ---------------------------------------------------------------------------

/// Create a minimal valid [`PropellerUnit`] for testing.
pub fn make_test_unit() -> PropellerUnit {
    make_test_unit_with_shard(vec![1, 2, 3])
}

/// Create a test unit with a specific shard payload.
pub fn make_test_unit_with_shard(shard: Vec<u8>) -> PropellerUnit {
    PropellerUnit::new(
        CommitteeId([1u8; 32]),
        PeerId::random(),
        MessageRoot([42u8; 32]),
        vec![0u8; 64],
        ShardIndex(0),
        ShardsOfPeer(vec![Shard(shard)]),
        MerkleProof { siblings: vec![[0u8; 32]] },
        0,
    )
}

// ---------------------------------------------------------------------------
// Construction helpers
// ---------------------------------------------------------------------------

pub fn make_config() -> Config {
    Config {
        stream_protocol: TEST_PROTOCOL,
        max_wire_message_size: MAX_WIRE_MESSAGE_SIZE,
        ..Config::default()
    }
}

pub fn make_handler() -> (Handler, futures::channel::mpsc::Receiver<PropellerUnit>) {
    let (tx, rx) = futures::channel::mpsc::channel(1024);
    (Handler::new(&make_config(), tx), rx)
}

pub fn make_handler_with_max_size(
    max_size: usize,
) -> (Handler, futures::channel::mpsc::Receiver<PropellerUnit>) {
    let (tx, rx) = futures::channel::mpsc::channel(1024);
    let config = Config { max_wire_message_size: max_size, ..make_config() };
    (Handler::new(&config, tx), rx)
}

pub fn remote_framed(stream: Stream) -> Framed<Stream, PropellerCodec> {
    Framed::new(stream, PropellerCodec::new(MAX_WIRE_MESSAGE_SIZE))
}

#[allow(dead_code)]
pub fn remote_framed_with_max_size(
    stream: Stream,
    max_size: usize,
) -> Framed<Stream, PropellerCodec> {
    Framed::new(stream, PropellerCodec::new(max_size))
}

// ---------------------------------------------------------------------------
// Simulation helpers — call handler methods as the swarm would in production
// ---------------------------------------------------------------------------

pub fn simulate_fully_negotiated_inbound(handler: &mut Handler, stream: Stream) {
    let framed = Framed::new(stream, PropellerCodec::new(MAX_WIRE_MESSAGE_SIZE));
    handler.on_connection_event(ConnectionEvent::FullyNegotiatedInbound(FullyNegotiatedInbound {
        protocol: framed,
        info: (),
    }));
}

pub fn simulate_fully_negotiated_outbound(handler: &mut Handler, stream: Stream, index: usize) {
    let framed = Framed::new(stream, PropellerCodec::new(MAX_WIRE_MESSAGE_SIZE));
    handler.on_connection_event(ConnectionEvent::FullyNegotiatedOutbound(
        FullyNegotiatedOutbound { protocol: framed, info: index },
    ));
}

pub fn simulate_dial_upgrade_error(
    handler: &mut Handler,
    index: usize,
    error: StreamUpgradeError<Infallible>,
) {
    handler.on_connection_event(ConnectionEvent::DialUpgradeError(DialUpgradeError {
        info: index,
        error,
    }));
}

pub fn simulate_send_unit(handler: &mut Handler, unit: PropellerUnit) {
    handler.on_behaviour_event(HandlerIn::SendUnit(unit));
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

pub async fn validate_received_unit(
    handler: &mut Handler,
    unit_rx: &mut futures::channel::mpsc::Receiver<PropellerUnit>,
    expected: &PropellerUnit,
) {
    use futures::StreamExt;
    let received = loop {
        tokio::select! {
            unit = unit_rx.next() => break unit.expect("Expected a unit on the channel"),
            _ = handler.next() => continue,
        }
    };
    assert_eq!(&received, expected);
}

pub async fn validate_send_error(handler: &mut Handler) {
    let event = handler.next().await.unwrap();
    assert_matches!(event, ConnectionHandlerEvent::NotifyBehaviour(HandlerOut::SendError(_)));
}

pub async fn validate_outbound_substream_request(handler: &mut Handler) {
    let event = handler.next().await.unwrap();
    assert_matches!(event, ConnectionHandlerEvent::OutboundSubstreamRequest { .. });
}

pub fn validate_no_events(handler: &mut Handler) {
    assert!(handler.next().now_or_never().is_none());
}

// ---------------------------------------------------------------------------
// Remote-peer helpers
// ---------------------------------------------------------------------------

pub async fn remote_send_units(
    remote: &mut Framed<Stream, PropellerCodec>,
    units: Vec<PropellerUnit>,
) {
    let batch = ProtoBatch { batch: units.into_iter().map(ProtoUnit::from).collect() };
    remote.send(batch).await.unwrap();
}

pub async fn remote_recv_batch(remote: &mut Framed<Stream, PropellerCodec>) -> ProtoBatch {
    remote.next().await.unwrap().unwrap()
}

pub async fn remote_send_raw_bytes(stream: &mut Stream, bytes: &[u8]) {
    stream.write_all(bytes).await.unwrap();
    stream.flush().await.unwrap();
}
