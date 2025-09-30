//! Tests for the `Handler` ensuring it requests outbound substreams and does not emit errors.

#![allow(clippy::as_conversions)]

use std::collections::VecDeque;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use apollo_propeller::{Handler, HandlerIn, HandlerOut, PropellerMessage};
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
    max_shard_size: usize,
    /// Substream timeout
    substream_timeout: Duration,
    /// Queue of events to yield to the swarm
    events: VecDeque<ToSwarm<HandlerTestEvent, HandlerIn>>,
}

/// Events emitted by the HandlerTestBehaviour
#[derive(Debug)]
pub enum HandlerTestEvent {
    /// A message was received from a peer
    MessageReceived { peer: PeerId, connection: ConnectionId, message: PropellerMessage },
    /// An error occurred while sending a message
    SendError { peer: PeerId, connection: ConnectionId, error: String },
}

impl HandlerTestBehaviour {
    /// Create a new HandlerTestBehaviour with default settings
    pub fn new() -> Self {
        Self::with_config(StreamProtocol::new("/propeller/1.0.0"), 1 << 20, Duration::from_secs(30))
    }

    /// Create a new HandlerTestBehaviour with custom configuration
    pub fn with_config(
        protocol_id: StreamProtocol,
        max_shard_size: usize,
        substream_timeout: Duration,
    ) -> Self {
        Self { protocol_id, max_shard_size, substream_timeout, events: VecDeque::new() }
    }

    /// Send a message to a specific peer on a specific connection
    pub fn send_message(&mut self, peer_id: PeerId, message: PropellerMessage) {
        self.events.push_front(ToSwarm::NotifyHandler {
            peer_id,
            handler: libp2p::swarm::NotifyHandler::Any,
            event: HandlerIn::SendMessage(message),
        });
    }
}

impl Default for HandlerTestBehaviour {
    fn default() -> Self {
        Self::new()
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
        Ok(Handler::new(self.protocol_id.clone(), self.max_shard_size, self.substream_timeout))
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
        _port_use: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(Handler::new(self.protocol_id.clone(), self.max_shard_size, self.substream_timeout))
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
            HandlerOut::Message(message) => {
                self.events.push_front(ToSwarm::GenerateEvent(HandlerTestEvent::MessageReceived {
                    peer: peer_id,
                    connection: connection_id,
                    message,
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

fn create_swarm(transport_type: TransportType) -> Swarm<HandlerTestBehaviour> {
    use libp2p::core::upgrade::Version;
    use libp2p::core::Transport as _;
    use libp2p::identity::Keypair;

    let identity = Keypair::generate_ed25519();
    let peer_id = PeerId::from(identity.public());

    let transport = match transport_type {
        TransportType::Memory => libp2p::core::transport::MemoryTransport::default()
            .or_transport(libp2p::tcp::tokio::Transport::default())
            .upgrade(Version::V1)
            .authenticate(libp2p::plaintext::Config::new(&identity))
            .multiplex(libp2p::yamux::Config::default())
            .timeout(Duration::from_secs(300))
            .boxed(),
        TransportType::Quic => {
            libp2p::quic::tokio::Transport::new(libp2p::quic::Config::new(&identity))
                .map(|(peer_id, muxer), _| {
                    (peer_id, libp2p::core::muxing::StreamMuxerBox::new(muxer))
                })
                .boxed()
        }
    };

    // Use a much longer idle connection timeout to prevent disconnections during long tests
    let swarm_config = libp2p::swarm::Config::with_tokio_executor()
        .with_idle_connection_timeout(Duration::from_secs(3600)); // 1 hour

    Swarm::new(transport, HandlerTestBehaviour::new(), peer_id, swarm_config)
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

// ****************************************************************************

async fn e2e(seed: u64, transport_type: TransportType) {
    use futures::StreamExt;
    use libp2p_swarm_test::SwarmExt;

    let mut rng = StdRng::seed_from_u64(seed);

    let num_messages = rng.gen_range(1..=10);
    let poll_bias = rng.gen_range(0.01..=0.99);
    let sender_bias = rng.gen_range(0.01..=0.99);

    // Create two swarms
    let mut swarm_1 = create_swarm(transport_type);
    let mut swarm_2 = create_swarm(transport_type);

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

    let original_message = PropellerMessage::random(&mut rng, 1 << 17);
    let mut sent = [0; 2];
    let mut received = [0; 2];
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);

    let mut swarms = [Box::pin(swarm_1), Box::pin(swarm_2)];
    let start_time = Instant::now();

    loop {
        let sent_sum = sent.iter().sum::<usize>();
        let received_sum = received.iter().sum::<usize>();
        if received_sum == num_messages {
            break;
        }
        if start_time.elapsed() > Duration::from_secs(5) {
            panic!("Test timed out");
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
                swarms[sender].behaviour_mut().send_message(peer, original_message.clone());
            }
            1 => {
                tokio::task::yield_now().await;
            }
            2 => {
                let polled = if rng.gen_bool(poll_bias) { 0 } else { 1 };
                match swarms[polled].poll_next_unpin(&mut cx) {
                    Poll::Ready(Some(event)) => match event {
                        SwarmEvent::Behaviour(HandlerTestEvent::MessageReceived {
                            peer,
                            connection: _,
                            message,
                        }) => {
                            if received_sum == num_messages {
                                break;
                            }
                            tracing::info!("Swarm {} message received: {:?}", polled, received_sum);
                            assert_eq!(message, original_message);
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

#[tokio::test]
async fn random_e2e_test_memory() {
    // let _ = tracing_subscriber::fmt().try_init();

    const NUM_TESTS: u64 = 1_000;
    for i in 0..NUM_TESTS {
        let seed = rand::random();
        println!("Running Memory test\t{}\twith seed\t{}", i, seed);
        e2e(seed, TransportType::Memory).await;
    }
}

#[tokio::test]
async fn random_e2e_test_quic() {
    // let _ = tracing_subscriber::fmt().try_init();

    const NUM_TESTS: u64 = 100;
    for i in 0..NUM_TESTS {
        let seed = rand::random();
        println!("Running QUIC test\t{}\twith seed\t{}", i, seed);
        e2e(seed, TransportType::Quic).await;
    }
}

#[tokio::test]
async fn specific_seed_random_e2e_test() {
    // let _ = tracing_subscriber::fmt().try_init();

    let seed = 5482653931163067145;
    e2e(seed, TransportType::Memory).await;
}
