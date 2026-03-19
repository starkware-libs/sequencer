//! Tests that demonstrate the lack of inbound back-pressure in the propeller stack.
//!
//! A custom FloodHandler writes raw PropellerUnitBatch frames as fast as yamux allows on a
//! propeller substream. The receiver runs the real propeller Behaviour. The test asserts that the
//! sender was able to write all batches without being slowed — documenting the vulnerability.
//!
//! When full back-pressure is implemented (handler + behaviour), the sender's writes will return
//! Pending (yamux window closes because the receiver stops reading) and the assertion should be
//! flipped to verify the sender was blocked.

use std::convert::Infallible;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use apollo_propeller::protocol::{PropellerCodec, PropellerProtocol};
use apollo_propeller::{Behaviour, Config};
use apollo_protobuf::protobuf::{
    Hash256 as ProtoHash256,
    MerkleProof as ProtoMerkleProof,
    PeerId as ProtoPeerId,
    PropellerUnit as ProtoUnit,
    PropellerUnitBatch as ProtoBatch,
    Shard as ProtoShard,
    ShardsOfPeer as ProtoShardsOfPeer,
};
use asynchronous_codec::Framed;
use futures::prelude::*;
use libp2p::core::transport::PortUse;
use libp2p::core::Endpoint;
use libp2p::swarm::handler::{ConnectionEvent, FullyNegotiatedOutbound};
use libp2p::swarm::{
    ConnectionDenied,
    ConnectionHandler,
    ConnectionHandlerEvent,
    ConnectionId,
    FromSwarm,
    NetworkBehaviour,
    SubstreamProtocol,
    ToSwarm,
};
use libp2p::{Multiaddr, PeerId, Swarm};
use libp2p_swarm_test::SwarmExt;

const TARGET_BATCHES: usize = 500;
const UNITS_PER_BATCH: usize = 10;
const TIMEOUT: Duration = Duration::from_secs(10);

/// Creates a `PropellerUnitBatch` containing `num_units` proto units that survive
/// `TryFrom<ProtoPropellerUnit>` conversion in the handler.
fn make_dummy_batch(num_units: usize, publisher: PeerId) -> ProtoBatch {
    let publisher_bytes = publisher.to_bytes();
    ProtoBatch {
        batch: (0..u64::try_from(num_units).expect("num_units overflows u64"))
            .map(|index| ProtoUnit {
                shards: Some(ProtoShardsOfPeer {
                    shards: vec![ProtoShard { data: vec![0u8; 64] }],
                }),
                index,
                merkle_root: Some(ProtoHash256 { elements: vec![0u8; 32] }),
                merkle_proof: Some(ProtoMerkleProof { siblings: vec![] }),
                publisher: Some(ProtoPeerId { id: publisher_bytes.clone() }),
                signature: vec![0u8; 64],
                committee_id: Some(ProtoHash256 { elements: vec![0u8; 32] }),
                nonce: index,
            })
            .collect(),
    }
}

/// State of the flood handler's outbound substream.
enum FloodOutboundState {
    /// Will request a substream on next poll.
    RequestSubstream,
    /// Waiting for substream negotiation to complete.
    Pending,
    /// Actively writing batches.
    Sending(Framed<libp2p::swarm::Stream, PropellerCodec>),
    /// Finished writing (either completed target or encountered error).
    Done,
}

/// A ConnectionHandler that floods a propeller substream with batches as fast as yamux allows.
struct FloodHandler {
    protocol: PropellerProtocol,
    outbound: FloodOutboundState,
    batches_sent: Arc<AtomicUsize>,
    publisher: PeerId,
}

impl FloodHandler {
    fn new(config: &Config, batches_sent: Arc<AtomicUsize>, publisher: PeerId) -> Self {
        let protocol =
            PropellerProtocol::new(config.stream_protocol.clone(), config.max_wire_message_size);
        Self { protocol, outbound: FloodOutboundState::RequestSubstream, batches_sent, publisher }
    }
}

impl ConnectionHandler for FloodHandler {
    type FromBehaviour = Infallible;
    type ToBehaviour = ();
    type InboundProtocol = PropellerProtocol;
    type OutboundProtocol = PropellerProtocol;
    type InboundOpenInfo = ();
    type OutboundOpenInfo = ();

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
        SubstreamProtocol::new(self.protocol.clone(), ())
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<
        ConnectionHandlerEvent<Self::OutboundProtocol, Self::OutboundOpenInfo, Self::ToBehaviour>,
    > {
        loop {
            match &mut self.outbound {
                FloodOutboundState::RequestSubstream => {
                    self.outbound = FloodOutboundState::Pending;
                    return Poll::Ready(ConnectionHandlerEvent::OutboundSubstreamRequest {
                        protocol: SubstreamProtocol::new(self.protocol.clone(), ()),
                    });
                }
                FloodOutboundState::Pending | FloodOutboundState::Done => return Poll::Pending,
                FloodOutboundState::Sending(_) => {}
            }

            // Take the framed stream out to satisfy borrow checker.
            let FloodOutboundState::Sending(framed) =
                std::mem::replace(&mut self.outbound, FloodOutboundState::Done)
            else {
                unreachable!();
            };
            let mut framed = framed;

            if self.batches_sent.load(Ordering::Relaxed) >= TARGET_BATCHES {
                // All batches written. Flush remaining data.
                match Sink::poll_flush(Pin::new(&mut framed), cx) {
                    Poll::Ready(_) => {
                        self.outbound = FloodOutboundState::Done;
                        return Poll::Pending;
                    }
                    Poll::Pending => {
                        self.outbound = FloodOutboundState::Sending(framed);
                        return Poll::Pending;
                    }
                }
            }

            match Sink::poll_ready(Pin::new(&mut framed), cx) {
                Poll::Ready(Ok(())) => {
                    let batch = make_dummy_batch(UNITS_PER_BATCH, self.publisher);
                    match Sink::start_send(Pin::new(&mut framed), batch) {
                        Ok(()) => {
                            self.batches_sent.fetch_add(1, Ordering::Relaxed);
                            self.outbound = FloodOutboundState::Sending(framed);
                            // Loop back to write more.
                        }
                        Err(_) => {
                            self.outbound = FloodOutboundState::Done;
                            return Poll::Pending;
                        }
                    }
                }
                Poll::Ready(Err(_)) => {
                    self.outbound = FloodOutboundState::Done;
                    return Poll::Pending;
                }
                Poll::Pending => {
                    // yamux is full — this is back-pressure working.
                    self.outbound = FloodOutboundState::Sending(framed);
                    return Poll::Pending;
                }
            }
        }
    }

    fn on_behaviour_event(&mut self, event: Self::FromBehaviour) {
        match event {}
    }

    fn on_connection_event(
        &mut self,
        event: ConnectionEvent<
            '_,
            Self::InboundProtocol,
            Self::OutboundProtocol,
            Self::InboundOpenInfo,
            Self::OutboundOpenInfo,
        >,
    ) {
        if let ConnectionEvent::FullyNegotiatedOutbound(FullyNegotiatedOutbound {
            protocol: framed,
            ..
        }) = event
        {
            self.outbound = FloodOutboundState::Sending(framed);
        }
    }
}

/// A NetworkBehaviour that creates FloodHandlers to flood connected peers.
struct FloodBehaviour {
    config: Config,
    batches_sent: Arc<AtomicUsize>,
}

impl FloodBehaviour {
    fn new(config: Config, batches_sent: Arc<AtomicUsize>) -> Self {
        Self { config, batches_sent }
    }
}

impl NetworkBehaviour for FloodBehaviour {
    type ConnectionHandler = FloodHandler;
    type ToSwarm = ();

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        Ok(FloodHandler::new(&self.config, self.batches_sent.clone(), PeerId::random()))
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
        _port_use: PortUse,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        Ok(FloodHandler::new(&self.config, self.batches_sent.clone(), PeerId::random()))
    }

    fn on_swarm_event(&mut self, _event: FromSwarm<'_>) {}

    fn on_connection_handler_event(
        &mut self,
        _peer_id: PeerId,
        _connection_id: ConnectionId,
        _event: (),
    ) {
    }

    fn poll(
        &mut self,
        _cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, <Self::ConnectionHandler as ConnectionHandler>::FromBehaviour>>
    {
        Poll::Pending
    }
}

/// Demonstrates that a peer can flood the propeller stack without being back-pressured.
///
/// The sender writes `TARGET_BATCHES` batches of `UNITS_PER_BATCH` units each on a propeller
/// substream. The receiver runs the real propeller Behaviour. Currently, all batches are accepted
/// without the sender being slowed — the handler reads everything eagerly and the behaviour
/// forwards to the engine via an unbounded channel.
///
/// When full back-pressure is implemented (handler caps inbound reads AND behaviour rate-limits
/// the handler), the sender's writes should return Pending as yamux flow control kicks in.
/// At that point, change the assertion to verify the sender was blocked.
#[tokio::test(flavor = "current_thread")]
async fn test_flood_is_not_back_pressured() {
    let config = Config::default();
    let batches_sent = Arc::new(AtomicUsize::new(0));

    let mut sender_swarm = Swarm::new_ephemeral_tokio({
        let config = config.clone();
        let batches_sent = batches_sent.clone();
        move |_keypair| FloodBehaviour::new(config, batches_sent)
    });

    let mut receiver_swarm =
        Swarm::new_ephemeral_tokio(|keypair| Behaviour::new(keypair.clone(), Config::default()));

    sender_swarm.listen().with_memory_addr_external().await;
    receiver_swarm.listen().with_memory_addr_external().await;
    sender_swarm.connect(&mut receiver_swarm).await;

    // Drive both swarms until the sender finishes or the timeout expires.
    let sender_driver = tokio::spawn(async move {
        loop {
            sender_swarm.select_next_some().await;
        }
    });
    let receiver_driver = tokio::spawn(async move {
        loop {
            receiver_swarm.select_next_some().await;
        }
    });

    // Wait for the sender to finish writing all batches, or timeout.
    let result = tokio::time::timeout(TIMEOUT, async {
        loop {
            if batches_sent.load(Ordering::Relaxed) >= TARGET_BATCHES {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;

    let final_count = batches_sent.load(Ordering::Relaxed);

    sender_driver.abort();
    receiver_driver.abort();

    // Current assertion: documents the vulnerability.
    // The sender wrote all batches without being back-pressured.
    // The handler eagerly reads everything from the wire, and the behaviour forwards it all
    // to the engine via an unbounded channel — no layer applies back-pressure.
    assert!(
        result.is_ok(),
        "Expected sender to complete all {TARGET_BATCHES} batches without back-pressure, but it \
         was blocked after {final_count} batches. If back-pressure has been implemented, update \
         this test to assert the sender IS blocked."
    );
    assert_eq!(
        final_count, TARGET_BATCHES,
        "Sender should have written exactly {TARGET_BATCHES} batches"
    );

    // TODO: After implementing full back-pressure (handler + behaviour), change to:
    // assert!(
    //     result.is_err(),
    //     "Expected sender to be back-pressured before completing {TARGET_BATCHES} batches, \
    //      but it completed all of them. Back-pressure is not working end-to-end."
    // );
}
