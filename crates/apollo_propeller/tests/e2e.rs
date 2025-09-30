//! End-to-end tests with large networks and publisher rotation.

use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use apollo_propeller::{Behaviour, Channel, Config, Event, MessageAuthenticity, MessageRoot};
use futures::stream::SelectAll;
use futures::StreamExt;
use libp2p::identity::PeerId;
use libp2p::swarm::{Swarm, SwarmEvent};
use libp2p_swarm_test::SwarmExt;
use rand::{Rng, SeedableRng};
use rstest::rstest;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

// ****************************************************************************

const CHANNEL: Channel = Channel(12);

// Multi-file writer that routes logs to different files per thread
struct MultiFileWriter {
    files: Arc<Mutex<HashMap<std::thread::ThreadId, Arc<Mutex<std::fs::File>>>>>,
}

impl MultiFileWriter {
    fn new() -> Self {
        Self { files: Arc::new(Mutex::new(HashMap::new())) }
    }

    fn add_file(&self, file: std::fs::File) {
        let thread_id = std::thread::current().id();
        self.files.lock().unwrap().insert(thread_id, Arc::new(Mutex::new(file)));
    }
}

impl Write for MultiFileWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let thread_id = std::thread::current().id();
        if let Some(file) = self.files.lock().unwrap().get(&thread_id) {
            file.lock().unwrap().write(buf)
        } else {
            Ok(buf.len()) // Ignore writes from threads without files
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let thread_id = std::thread::current().id();
        if let Some(file) = self.files.lock().unwrap().get(&thread_id) {
            file.lock().unwrap().flush()
        } else {
            Ok(())
        }
    }
}

impl Clone for MultiFileWriter {
    fn clone(&self) -> Self {
        Self { files: Arc::clone(&self.files) }
    }
}

static MULTI_WRITER: Mutex<Option<MultiFileWriter>> = Mutex::new(None);

// ****************************************************************************

/// Transport type for testing
#[derive(Debug, Clone, Copy)]
enum TransportType {
    Memory,
    Quic,
}

fn create_swarm(transport_type: TransportType) -> Swarm<Behaviour> {
    use libp2p::identity::Keypair;

    let config = Config::builder().build();

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
            .with_behaviour(|identity| {
                Behaviour::new(MessageAuthenticity::Signed(identity.clone()), config)
            })
            .expect("Failed to create behaviour")
            .with_swarm_config(|c| {
                // Use a much longer idle connection timeout to prevent disconnections during long
                // tests
                c.with_idle_connection_timeout(Duration::from_secs(3600)) // 1 hour
            })
            .build(),
        TransportType::Quic => builder
            .with_quic()
            .with_behaviour(|identity| {
                Behaviour::new(MessageAuthenticity::Signed(identity.clone()), config)
            })
            .expect("Failed to create behaviour")
            .with_swarm_config(|c| {
                // Use a much longer idle connection timeout to prevent disconnections during long
                // tests
                c.with_idle_connection_timeout(Duration::from_secs(3600)) // 1 hour
            })
            .build(),
    }
}

async fn setup_network(
    num_nodes: usize,
    transport_type: TransportType,
) -> (Vec<Swarm<Behaviour>>, Vec<PeerId>) {
    let mut swarms = Vec::with_capacity(num_nodes);
    let mut peer_ids = Vec::with_capacity(num_nodes);

    for _ in 0..num_nodes {
        let mut swarm = create_swarm(transport_type);
        let peer_id = *swarm.local_peer_id();

        match transport_type {
            TransportType::Memory => {
                swarm.listen().with_memory_addr_external().await;
            }
            TransportType::Quic => {
                swarm.listen_on("/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap()).unwrap();
                // Wait for the listening event and add as external address
                loop {
                    if let SwarmEvent::NewListenAddr { address, .. } =
                        swarm.select_next_some().await
                    {
                        swarm.add_external_address(address);
                        break;
                    }
                }
            }
        }

        peer_ids.push(peer_id);
        swarms.push(swarm);
    }

    tracing::info!("🔗 Connecting all peers");
    connect_all_peers(&mut swarms).await;
    tracing::info!("🔗 Adding all peers");
    add_all_peers(&mut swarms, &peer_ids).await;

    (swarms, peer_ids)
}

async fn connect_all_peers(swarms: &mut [Swarm<Behaviour>]) {
    let num_nodes = swarms.len();

    for i in 0..num_nodes {
        for j in (i + 1)..num_nodes {
            let (left, right) = swarms.split_at_mut(j);
            let swarm_i = &mut left[i];
            let swarm_j = &mut right[0];

            tracing::info!("connecting swarm {i} to {j}");
            swarm_j.connect(swarm_i).await;
        }
    }
}

async fn add_all_peers(swarms: &mut [Swarm<Behaviour>], peer_ids: &[PeerId]) {
    let peer_weights: Vec<(PeerId, u64)> =
        peer_ids.iter().map(|&peer_id| (peer_id, 1000)).collect();

    for swarm in swarms.iter_mut() {
        let _ = swarm.behaviour_mut().register_channel_peers(CHANNEL, peer_weights.clone()).await;
    }
}

async fn collect_message_events(
    swarms: &mut [Swarm<Behaviour>],
    expected_message_ids: Vec<MessageRoot>,
    number_of_messages: usize,
    publisher_idx: usize,
) -> HashMap<(usize, MessageRoot), Vec<u8>> {
    let mut received_messages: HashMap<(usize, MessageRoot), Vec<u8>> = HashMap::new();
    tracing::info!("🔍 Collecting events, need {} messages", number_of_messages);

    // Create a SelectAll to efficiently poll all swarm streams
    let mut select_all = SelectAll::new();

    // Add each swarm's stream with its index
    for (node_idx, swarm) in swarms.iter_mut().enumerate() {
        let stream = swarm.map(move |event| (node_idx, event));
        select_all.push(stream);
    }

    while let Some((node_idx, swarm_event)) = select_all.next().await {
        if let Ok(event) = swarm_event.try_into_behaviour_event() {
            match event {
                Event::MessageReceived { publisher: _, message_root, message: data } => {
                    let message_id = message_root;
                    if !expected_message_ids.contains(&message_id) {
                        continue;
                    }
                    if received_messages.contains_key(&(node_idx, message_id)) {
                        panic!(
                            "🚨 DUPLICATE MESSAGE: Node {} received a duplicate message! This \
                             should not happen. message_id: {}",
                            node_idx, message_id
                        );
                    }
                    assert!(received_messages.len() < number_of_messages);
                    assert_ne!(node_idx, publisher_idx);
                    received_messages.insert((node_idx, message_id), data);
                    tracing::info!(
                        "📨 Node {} message_id={} (received_messages={}/{})",
                        node_idx,
                        message_id,
                        received_messages.len(),
                        number_of_messages,
                    );
                }
                e => panic!("Unexpected event: {:?}", e),
            }
        }
        if received_messages.len() == number_of_messages {
            break;
        } else {
            tracing::info!(
                "🔍 Collecting events, need {} messages, received {} messages",
                number_of_messages,
                received_messages.len(),
            );
        }
    }

    received_messages
}

async fn broadcast_message(
    swarms: &mut [Swarm<Behaviour>],
    publisher_idx: usize,
    test_message: &[u8],
) -> MessageRoot {
    tracing::info!(
        "📡 Publisher {} broadcasting message of {} bytes",
        publisher_idx,
        test_message.len()
    );
    let message_root = swarms[publisher_idx]
        .behaviour_mut()
        .broadcast(CHANNEL, test_message.to_vec())
        .await
        .unwrap();
    message_root
}

fn verify_received_data(
    received_messages: HashMap<(usize, MessageRoot), Vec<u8>>,
    test_messages: &HashMap<MessageRoot, Vec<u8>>,
    publisher_idx: usize,
) {
    for ((node_idx, message_id), message) in received_messages {
        let test_message = test_messages.get(&message_id).unwrap();
        assert_eq!(
            &message, test_message,
            "Node {} received incorrect reconstructed message from publisher {}: message_id={}",
            node_idx, publisher_idx, message_id
        );
    }
}

fn assert_collection_counts(
    received_messages: &HashMap<(usize, MessageRoot), Vec<u8>>,
    expected_messages: usize,
) {
    assert_eq!(received_messages.len(), expected_messages);
}

#[allow(clippy::too_many_arguments)]
async fn broadcast_and_verify_burst(
    swarms: &mut [Swarm<Behaviour>],
    test_messages: &[Vec<u8>],
    publisher_idx: usize,
    num_nodes: usize,
) {
    tracing::info!("📤 Broadcasting all messages in burst mode");
    let mut message_roots = Vec::new();

    for test_message in test_messages.iter() {
        let message_root = broadcast_message(swarms, publisher_idx, test_message).await;
        message_roots.push(message_root);
    }

    tracing::info!("⏳ Collecting message events for publisher {}...", publisher_idx);
    let received_messages = collect_message_events(
        swarms,
        message_roots.clone(),
        message_roots.len() * (num_nodes - 1),
        publisher_idx,
    )
    .await;

    assert_collection_counts(&received_messages, message_roots.len() * (num_nodes - 1));

    verify_received_data(
        received_messages,
        &test_messages
            .iter()
            .zip(message_roots)
            .map(|(message, message_root)| (message_root, message.clone()))
            .collect(),
        publisher_idx,
    );
}

async fn broadcast_and_verify_sequential(
    swarms: &mut [Swarm<Behaviour>],
    test_messages: &[Vec<u8>],
    publisher_idx: usize,
    num_nodes: usize,
) {
    tracing::info!("📤 Broadcasting messages sequentially with verification");

    for test_message in test_messages.iter() {
        broadcast_and_verify_burst(
            swarms,
            std::slice::from_ref(test_message),
            publisher_idx,
            num_nodes,
        )
        .await;
    }
}

async fn e2e(
    num_nodes: usize,
    number_of_messages: usize,
    number_of_publishers: usize,
    message_size: usize,
    send_in_burst: bool,
    transport_type: TransportType,
) {
    let (mut swarms, peer_ids) = setup_network(num_nodes, transport_type).await;
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);

    for publisher_idx in (0..num_nodes).step_by(num_nodes / number_of_publishers) {
        tracing::info!("🔄 Starting rotation to publisher {}", publisher_idx);

        let publisher_peer_id = peer_ids[publisher_idx];
        tracing::info!("🎯 Setting publisher to peer_id: {}", publisher_peer_id);
        tracing::info!("✅ Publisher {} confirmed", publisher_idx);

        tracing::info!("🔄 Creating test messages");
        // Store messages as a Vec since MessageRoot is derived from content
        let mut test_messages = Vec::new();
        for _ in 0..number_of_messages {
            let message: Vec<_> = (0..message_size).map(|_| rng.gen::<u8>()).collect();
            test_messages.push(message);
        }

        // Check connection status before broadcasting
        let connected_count = swarms[publisher_idx].connected_peers().count();
        tracing::info!("🔗 Publisher {} has {} connected peers", publisher_idx, connected_count);

        if send_in_burst {
            broadcast_and_verify_burst(&mut swarms, &test_messages, publisher_idx, num_nodes).await;
        } else {
            broadcast_and_verify_sequential(&mut swarms, &test_messages, publisher_idx, num_nodes)
                .await;
        }

        tracing::info!("✅ ✅ ✅ Publisher {} broadcast successful", publisher_idx);
    }
}

/// Initialize the tracing subscriber to write to a per-test file
fn init_tracing_to_file(test_name: &str) {
    let log_file = format!("/tmp/test_logs_{}.txt", test_name);
    let file = std::fs::File::create(&log_file).expect("Failed to create log file");

    eprintln!("Test logs: {}", log_file);

    // Initialize the global subscriber once
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let writer = MultiFileWriter::new();
        *MULTI_WRITER.lock().unwrap() = Some(writer.clone());

        tracing_subscriber::registry()
            .with(
                EnvFilter::builder()
                    .with_default_directive(LevelFilter::INFO.into())
                    .from_env_lossy(),
            )
            .with(
                tracing_subscriber::fmt::layer()
                    .with_writer(move || writer.clone())
                    .with_ansi(false)
                    .with_thread_ids(true)
                    .with_line_number(true),
            )
            .init();
    });

    // Register this test's file with the multi-writer
    if let Some(writer) = MULTI_WRITER.lock().unwrap().as_ref() {
        writer.add_file(file);
    }
}

#[tokio::test]
async fn test_increasing_network_sizes() {
    init_tracing_to_file("test_increasing_network_sizes");
    for num_nodes in 2..=31 {
        e2e(num_nodes, 1, 1, 1024, false, TransportType::Memory).await;
    }
}

#[tokio::test]
async fn random_e2e_test_memory() {
    init_tracing_to_file("random_e2e_test_memory");
    const NUM_TESTS: u64 = 10;
    for i in 0..NUM_TESTS {
        let seed = rand::random();
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let num_nodes = rng.gen_range(2..=100);
        let number_of_messages = rng.gen_range(1..=2);
        let number_of_publishers = rng.gen_range(1..=2);
        let message_size = rng.gen_range(256..=2048);
        let send_in_burst = rng.gen_bool(0.5);
        println!(
            "Memory {}: Running test with seed {}: num_nodes={}, number_of_messages={}, \
             number_of_publishers={}, message_size={}, send_in_burst={}",
            i,
            seed,
            num_nodes,
            number_of_messages,
            number_of_publishers,
            message_size,
            send_in_burst,
        );
        e2e(
            num_nodes,
            number_of_messages,
            number_of_publishers,
            message_size,
            send_in_burst,
            TransportType::Memory,
        )
        .await;
    }
}

#[tokio::test]
async fn random_e2e_test_quic() {
    init_tracing_to_file("random_e2e_test_quic");
    const NUM_TESTS: u64 = 10;
    for i in 0..NUM_TESTS {
        let seed = rand::random();
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let num_nodes = rng.gen_range(2..=10); // quic is slower
        let number_of_messages = rng.gen_range(1..=2);
        let number_of_publishers = rng.gen_range(1..=2);
        let message_size = rng.gen_range(256..=1024);
        let send_in_burst = rng.gen_bool(0.5);
        println!(
            "QUIC {}: Running test with seed {}: num_nodes={}, number_of_messages={}, \
             number_of_publishers={}, message_size={}, send_in_burst={}",
            i,
            seed,
            num_nodes,
            number_of_messages,
            number_of_publishers,
            message_size,
            send_in_burst,
        );
        e2e(
            num_nodes,
            number_of_messages,
            number_of_publishers,
            message_size,
            send_in_burst,
            TransportType::Quic,
        )
        .await;
    }
}

#[tokio::test]
#[rstest]
#[case(1<<10, 100)]
#[case(1<<11, 100)]
#[case(1<<12, 100)]
#[case(1<<13, 100)]
#[case(1<<14, 100)]
#[case(1<<15, 100)]
#[case(1<<16, 100)]
#[case(1<<17, 100)]
#[case(1<<18, 100)]
#[case(1<<19, 10)]
#[case(1<<20, 10)]
#[case(1<<21, 10)]
#[case(1<<22, 10)]
#[case(1<<23, 10)]
async fn specific_e2e_message_sizes(#[case] message_size: usize, #[case] num_nodes: usize) {
    init_tracing_to_file(&format!("specific_e2e_message_sizes_{}_{}", message_size, num_nodes));

    e2e(
        num_nodes,
        1, // number_of_messages
        1, // number_of_publishers
        message_size,
        true, // send_in_burst
        TransportType::Memory,
    )
    .await;
}
