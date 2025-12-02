//! Tests for the `Handler` ensuring it requests outbound substreams and does not emit errors.

#![allow(clippy::as_conversions)]

use std::collections::VecDeque;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use apollo_propeller::{
    Channel,
    Handler,
    HandlerIn,
    HandlerOut,
    MerkleProof,
    MessageRoot,
    PropellerUnit,
    ShardIndex,
};
use futures::task::noop_waker;
use libp2p::core::transport::PortUse;
use libp2p::core::{Endpoint, Multiaddr};
use libp2p::identity::PeerId;
use libp2p::swarm::behaviour::FromSwarm;
use libp2p::swarm::{
    ConnectionDenied,
    ConnectionId,
    NetworkBehaviour,
    StreamProtocol,
    Swarm,
    SwarmEvent,
    THandler,
    THandlerInEvent,
    THandlerOutEvent,
    ToSwarm,
};
use libp2p_swarm_test::SwarmExt;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

// ****************************************************************************

/// Transport type for testing
#[derive(Debug, Clone, Copy)]
enum TransportType {
    Memory,
    Quic,
}

// ****************************************************************************

// ****************************************************************************

/// A simple NetworkBehaviour that uses propeller::Handler for testing.
/// Provides a straightforward API to send and receive messages.
pub struct HandlerTestBehaviour {
    /// Protocol configuration
    protocol_id: StreamProtocol,
    /// Maximum shard size
    max_wire_message_size: usize,
    /// Substream timeout
    substream_timeout: Duration,
    /// Queue of events to yield to the swarm
    events: VecDeque<ToSwarm<HandlerTestEvent, HandlerIn>>,
}

/// Events emitted by the HandlerTestBehaviour
#[derive(Debug)]
pub enum HandlerTestEvent {
    /// A message was received from a peer
    UnitReceived { peer: PeerId, connection: ConnectionId, unit: PropellerUnit },
    /// An error occurred while sending a message
    SendError { peer: PeerId, connection: ConnectionId, error: String },
}

impl HandlerTestBehaviour {
    /// Create a new HandlerTestBehaviour with default settings
    pub fn new(max_wire_message_size: usize) -> Self {
        Self::with_config(
            StreamProtocol::new("/propeller/1.0.0"),
            max_wire_message_size,
            Duration::from_secs(30),
        )
    }

    /// Create a new HandlerTestBehaviour with custom configuration
    pub fn with_config(
        protocol_id: StreamProtocol,
        max_wire_message_size: usize,
        substream_timeout: Duration,
    ) -> Self {
        Self { protocol_id, max_wire_message_size, substream_timeout, events: VecDeque::new() }
    }

    /// Send a message to a specific peer on a specific connection
    pub fn send_unit(&mut self, peer_id: PeerId, unit: PropellerUnit) {
        self.events.push_front(ToSwarm::NotifyHandler {
            peer_id,
            handler: libp2p::swarm::NotifyHandler::Any,
            event: HandlerIn::SendUnit(unit),
        });
    }
}

impl NetworkBehaviour for HandlerTestBehaviour {
    type ConnectionHandler = Handler;
    type ToSwarm = HandlerTestEvent;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(Handler::new(
            self.protocol_id.clone(),
            self.max_wire_message_size,
            self.substream_timeout,
        ))
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
        _port_use: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(Handler::new(
            self.protocol_id.clone(),
            self.max_wire_message_size,
            self.substream_timeout,
        ))
    }

    fn on_swarm_event(&mut self, _event: FromSwarm<'_>) {
        // No special handling needed for swarm events in this test behaviour
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        match event {
            HandlerOut::Unit(unit) => {
                self.events.push_front(ToSwarm::GenerateEvent(HandlerTestEvent::UnitReceived {
                    peer: peer_id,
                    connection: connection_id,
                    unit,
                }));
            }
            HandlerOut::SendError(error) => {
                self.events.push_front(ToSwarm::GenerateEvent(HandlerTestEvent::SendError {
                    peer: peer_id,
                    connection: connection_id,
                    error,
                }));
            }
        }
    }

    fn poll(
        &mut self,
        _cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if let Some(event) = self.events.pop_back() {
            return Poll::Ready(event);
        }

        Poll::Pending
    }
}

// ****************************************************************************

fn create_swarm(
    transport_type: TransportType,
    max_wire_message_size: usize,
) -> Swarm<HandlerTestBehaviour> {
    use libp2p::identity::Keypair;

    let identity = Keypair::generate_ed25519();

    let builder = libp2p::SwarmBuilder::with_existing_identity(identity).with_tokio();

    match transport_type {
        TransportType::Memory => builder
            .with_other_transport(|keypair| {
                use libp2p::core::upgrade::Version;
                use libp2p::core::Transport as _;

                libp2p::core::transport::MemoryTransport::default()
                    .or_transport(libp2p::tcp::tokio::Transport::default())
                    .upgrade(Version::V1)
                    .authenticate(libp2p::plaintext::Config::new(keypair))
                    .multiplex(libp2p::yamux::Config::default())
                    .timeout(Duration::from_secs(300))
                    .boxed()
            })
            .expect("Failed to build transport")
            .with_behaviour(|_| HandlerTestBehaviour::new(max_wire_message_size))
            .expect("Failed to create behaviour")
            .with_swarm_config(|c| {
                // Use a much longer idle connection timeout to prevent disconnections during long
                // tests
                c.with_idle_connection_timeout(Duration::from_secs(3600)) // 1 hour
            })
            .build(),
        TransportType::Quic => builder
            .with_quic()
            .with_behaviour(|_| HandlerTestBehaviour::new(max_wire_message_size))
            .expect("Failed to create behaviour")
            .with_swarm_config(|c| {
                // Use a much longer idle connection timeout to prevent disconnections during long
                // tests
                c.with_idle_connection_timeout(Duration::from_secs(3600)) // 1 hour
            })
            .build(),
    }
}

async fn listen(swarm: &mut Swarm<HandlerTestBehaviour>, transport_type: TransportType) {
    use futures::StreamExt;

    match transport_type {
        TransportType::Memory => {
            swarm.listen().with_memory_addr_external().await;
        }
        TransportType::Quic => {
            swarm.listen_on("/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap()).unwrap();
            // Wait for the listening event and add as external address
            loop {
                if let SwarmEvent::NewListenAddr { address, .. } = swarm.select_next_some().await {
                    swarm.add_external_address(address);
                    break;
                }
            }
        }
    }
}

/// Initialize the tracing subscriber with error detection
fn init_tracing() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        tracing_subscriber::registry()
            .with(
                EnvFilter::builder()
                    .with_default_directive(LevelFilter::WARN.into())
                    .from_env_lossy(),
            )
            .with(tracing_subscriber::fmt::layer())
            .init();
    });
}

// ****************************************************************************

async fn e2e(seed: u64, transport_type: TransportType) {
    use futures::StreamExt;
    use libp2p_swarm_test::SwarmExt;

    let mut rng = StdRng::seed_from_u64(seed);

    let num_messages = rng.gen_range(1..=10);
    let poll_bias = rng.gen_range(0.01..=0.99);
    let sender_bias = rng.gen_range(0.01..=0.99);
    let max_shard_size = 1 << rng.gen_range(1..=24);

    // Create two swarms with enough buffer for all message overhead:
    // - root (32) + publisher (38) + signature (256) + index (4) + proof (256*32=8192)
    // - protobuf overhead (~100 bytes for tags and lengths)
    // Total: ~10KB overhead
    let max_wire_size = apollo_propeller::Config::default().max_wire_message_size();
    let mut swarm_1 = create_swarm(transport_type, max_wire_size);
    let mut swarm_2 = create_swarm(transport_type, max_wire_size);

    let peer_id_1 = *swarm_1.local_peer_id();
    let peer_id_2 = *swarm_2.local_peer_id();
    let peers = [peer_id_1, peer_id_2];

    // Set up listening and connect
    listen(&mut swarm_1, transport_type).await;
    listen(&mut swarm_2, transport_type).await;

    if rng.gen_bool(0.5) {
        swarm_1.connect(&mut swarm_2).await;
    } else {
        swarm_2.connect(&mut swarm_1).await;
    }
    assert!(swarm_1.is_connected(&peers[1]));
    assert!(swarm_2.is_connected(&peers[0]));

    let original_unit = PropellerUnit::random(&mut rng, max_shard_size);
    let mut sent = [0; 2];
    let mut received = [0; 2];
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);

    let mut swarms = [Box::pin(swarm_1), Box::pin(swarm_2)];
    let start_time = Instant::now();

    tracing::info!(
        "Starting test: num_messages={}, poll_bias={:.2}, sender_bias={:.2}, max_shard_size={}",
        num_messages,
        poll_bias,
        sender_bias,
        max_shard_size
    );

    for loop_count in 0.. {
        let sent_sum = sent.iter().sum::<usize>();
        let received_sum = received.iter().sum::<usize>();

        if loop_count % 1000 == 0 {
            tracing::info!("Loop {}: sent={}, received={}", loop_count, sent_sum, received_sum);
        }

        if received_sum == num_messages {
            break;
        }
        if start_time.elapsed() > Duration::from_secs(10) {
            panic!(
                "Test timed out after {} loops: sent={}, received={}",
                loop_count, sent_sum, received_sum
            );
        }

        match rng.gen_range(0..=2) {
            0 => {
                if sent_sum == num_messages {
                    continue;
                }
                let sender = if rng.gen_bool(sender_bias) { 0 } else { 1 };
                tracing::info!("Swarm {} sending message {}", sender, sent_sum);
                let receiver = 1 - sender;
                let peer = peers[receiver];
                sent[sender] += 1;
                swarms[sender].behaviour_mut().send_unit(peer, original_unit.clone());
            }
            1 => {
                tokio::task::yield_now().await;
            }
            2 => {
                let polled = if rng.gen_bool(poll_bias) { 0 } else { 1 };
                match swarms[polled].poll_next_unpin(&mut cx) {
                    Poll::Ready(Some(event)) => match event {
                        SwarmEvent::Behaviour(HandlerTestEvent::UnitReceived {
                            peer,
                            connection: _,
                            unit,
                        }) => {
                            if received_sum == num_messages {
                                break;
                            }
                            tracing::info!("Swarm {} message received: {:?}", polled, received_sum);
                            assert_eq!(unit, original_unit);
                            assert_eq!(peer, peers[1 - polled]);
                            received[polled] += 1;
                        }
                        SwarmEvent::Behaviour(HandlerTestEvent::SendError {
                            peer,
                            connection: _,
                            error,
                        }) => {
                            panic!("Send error from peer {:?}: {}", peer, error);
                        }
                        e => panic!("Unexpected event: {:?}", e),
                    },
                    Poll::Ready(None) => {
                        panic!("Swarm {} ended", polled);
                    }
                    Poll::Pending => {}
                }
            }
            _ => unreachable!(),
        }
    }

    let sent_sum = sent.iter().sum::<usize>();
    let received_sum = received.iter().sum::<usize>();
    assert_eq!(received_sum, sent_sum);
    assert_eq!(received[0], sent[1]);
    assert_eq!(received[1], sent[0]);
}

// ****************************************************************************

#[tokio::test]
async fn random_e2e_test_memory() {
    init_tracing();
    const NUM_TESTS: u64 = 1_000;
    for i in 0..NUM_TESTS {
        let seed = rand::random();
        println!("Running Memory test\t{}\twith seed\t{}", i, seed);
        e2e(seed, TransportType::Memory).await;
    }
}

#[tokio::test]
async fn random_e2e_test_quic() {
    init_tracing();
    const NUM_TESTS: u64 = 100;
    for i in 0..NUM_TESTS {
        let seed = rand::random();
        println!("Running QUIC test\t{}\twith seed\t{}", i, seed);
        e2e(seed, TransportType::Quic).await;
    }
}

#[tokio::test]
async fn specific_seed_random_e2e_test() {
    init_tracing();
    let seed = 74093889254187274;
    e2e(seed, TransportType::Memory).await;
}

#[tokio::test]
#[ignore]
async fn handler_performance_test() {
    init_tracing();
    let transport_type = TransportType::Quic;

    for shard_size in 10..17 {
        let shard_size = 1 << shard_size;
        let num_messages = (10000 * (1 << 17)) / shard_size;

        use libp2p_swarm_test::SwarmExt;

        // Create two swarms with enough buffer for message overhead
        let max_wire_size = shard_size + (1 << 14); // +16KB for overhead
        let mut swarm_1 = create_swarm(transport_type, max_wire_size);
        let mut swarm_2 = create_swarm(transport_type, max_wire_size);

        let peer_id_1 = *swarm_1.local_peer_id();
        let peer_id_2 = *swarm_2.local_peer_id();
        let peers = [peer_id_1, peer_id_2];

        // Set up listening and connect
        listen(&mut swarm_1, transport_type).await;
        listen(&mut swarm_2, transport_type).await;

        swarm_1.connect(&mut swarm_2).await;
        assert!(swarm_1.is_connected(&peers[1]));
        assert!(swarm_2.is_connected(&peers[0]));

        let original_message = PropellerUnit::new(
            Channel(0),
            peer_id_2,
            MessageRoot([0; 32]),
            vec![],
            ShardIndex(0),
            vec![0; shard_size],
            MerkleProof { siblings: vec![] },
        );

        let mut received = 0;

        for _ in 0..num_messages {
            swarm_2.behaviour_mut().send_unit(peer_id_1, original_message.clone());
        }
        let start_time = Instant::now();
        while received < num_messages {
            tokio::select! {
                event = swarm_1.next_swarm_event() => {
                    match event {
                        SwarmEvent::Behaviour(HandlerTestEvent::UnitReceived {
                            peer: _,
                            connection: _,
                            unit: _,
                        }) => {
                            received += 1;
                        }
                        _ => {
                            unreachable!();
                        }
                    }
                },
                event = swarm_2.next_swarm_event() => {
                    unreachable!("{:?}", event);
                },
            }
        }
        let total_bytes = shard_size * num_messages;
        let duration = start_time.elapsed();
        println!(
            "Message size: {} bytes, Time taken: {:?}, Throughput: {:?} MB/s",
            shard_size,
            duration,
            total_bytes as f64 / duration.as_secs_f64() / 1024.0 / 1024.0,
        );
    }
}
