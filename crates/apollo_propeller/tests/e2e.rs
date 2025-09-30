//! End-to-end tests with large networks and publisher rotation.

use std::collections::HashMap;
use std::time::Duration;

use apollo_propeller::{Behaviour, Config, Event, MessageAuthenticity, MessageRoot, ShardIndex};
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

// Type alias for compatibility with test code
type MessageId = MessageRoot;

// ****************************************************************************

/// Transport type for testing
#[derive(Debug, Clone, Copy)]
enum TransportType {
    Memory,
    Quic,
}

async fn create_swarm(transport_type: TransportType) -> Swarm<Behaviour> {
    use libp2p::core::upgrade::Version;
    use libp2p::core::Transport as _;
    use libp2p::identity::Keypair;

    let config = Config::builder()
        .emit_shard_received_events(true)
        .validation_mode(apollo_propeller::ValidationMode::None)
        .max_shard_size(1 << 24) // 16MB
        .message_cache_ttl(Duration::from_secs(3600)) // 1 hour
        .substream_timeout(Duration::from_secs(300)) // Increased timeout for large message tests
        .build();

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
            let mut quic_config = libp2p::quic::Config::new(&identity);
            quic_config.max_connection_data = u32::MAX;
            quic_config.max_stream_data = u32::MAX;
            quic_config.keep_alive_interval = Duration::from_secs(300);
            quic_config.max_idle_timeout = 300 * 1000;
            libp2p::quic::tokio::Transport::new(quic_config)
                .map(|(peer_id, muxer), _| {
                    (peer_id, libp2p::core::muxing::StreamMuxerBox::new(muxer))
                })
                .boxed()
        }
    };

    // Use a much longer idle connection timeout to prevent disconnections during long tests
    let swarm_config = libp2p::swarm::Config::with_tokio_executor()
        .with_idle_connection_timeout(Duration::from_secs(3600)); // 1 hour

    Swarm::new(
        transport,
        Behaviour::new(MessageAuthenticity::Signed(identity), config),
        peer_id,
        swarm_config,
    )
}

async fn setup_network(
    num_nodes: usize,
    transport_type: TransportType,
) -> (Vec<Swarm<Behaviour>>, Vec<PeerId>) {
    let mut swarms = Vec::with_capacity(num_nodes);
    let mut peer_ids = Vec::with_capacity(num_nodes);

    for _ in 0..num_nodes {
        let mut swarm = create_swarm(transport_type).await;
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
    add_all_peers(&mut swarms, &peer_ids);

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

fn add_all_peers(swarms: &mut [Swarm<Behaviour>], peer_ids: &[PeerId]) {
    let peer_weights: Vec<(PeerId, u64)> =
        peer_ids.iter().map(|&peer_id| (peer_id, 1000)).collect();

    for swarm in swarms.iter_mut() {
        let _ = swarm.behaviour_mut().set_peers(peer_weights.clone());
    }
}

async fn collect_message_events(
    swarms: &mut [Swarm<Behaviour>],
    expected_message_ids: Vec<MessageId>,
    number_of_messages: usize,
    number_of_shards: usize,
    publisher_idx: usize,
    early_stop: bool,
) -> (HashMap<(usize, MessageId), Vec<u8>>, HashMap<(usize, MessageId, ShardIndex), Vec<u8>>) {
    let mut received_messages: HashMap<(usize, MessageId), Vec<u8>> = HashMap::new();
    let mut received_shards: HashMap<(usize, MessageId, ShardIndex), Vec<u8>> = HashMap::new();
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
                Event::ShardReceived {
                    publisher: _,
                    shard_index,
                    message_root,
                    sender: _,
                    shard,
                } => {
                    let message_id = message_root;
                    let index = shard_index.0;
                    if !expected_message_ids.contains(&message_id) {
                        continue;
                    }
                    if received_shards.contains_key(&(node_idx, message_id, shard_index)) {
                        panic!(
                            "🚨 DUPLICATE SHARD: Node {} received a duplicate shard! This should \
                             not happen. message_id={}, index={}",
                            node_idx, message_id, index
                        );
                    }
                    received_shards.insert((node_idx, message_id, shard_index), shard);
                    tracing::info!(
                        "📨 Node {} received shard for message_id={} index={} ({}/{})",
                        node_idx,
                        message_id,
                        index,
                        received_shards.len(),
                        number_of_shards,
                    );
                }
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
                        "📨 Node {} received message {} ({}/{})",
                        node_idx,
                        message_id,
                        received_messages.len(),
                        number_of_messages
                    );
                    if received_messages.len() == number_of_messages && early_stop {
                        break;
                    }
                }
                Event::ShardSendFailed { sent_from: _, sent_to, error } => {
                    panic!(
                        "Node {} failed to send shard to peer {:?}: {}",
                        node_idx, sent_to, error
                    );
                }
                Event::ShardValidationFailed {
                    sender,
                    claimed_root,
                    claimed_publisher: _,
                    error,
                } => {
                    panic!(
                        "Node {} failed to verify shard from peer {}: root={}, error={}",
                        node_idx, sender, claimed_root, error
                    );
                }
                Event::MessageReconstructionFailed { message_root, publisher, error } => {
                    let message_id = message_root;
                    panic!(
                        "Node {} failed to reconstruct message from shards: publisher={}, \
                         message_id={}, error={}",
                        node_idx, publisher, message_id, error
                    );
                }
            }
        }
        if number_of_shards == received_shards.len()
            && number_of_messages == received_messages.len()
        {
            break;
        }
    }

    (received_messages, received_shards)
}

fn broadcast_message(
    swarms: &mut [Swarm<Behaviour>],
    publisher_idx: usize,
    test_message: &[u8],
) -> (MessageId, HashMap<ShardIndex, Vec<u8>>) {
    tracing::info!(
        "📡 Publisher {} broadcasting message of {} bytes",
        publisher_idx,
        test_message.len()
    );

    let test_shards =
        swarms[publisher_idx].behaviour_mut().prepare_messages(test_message.to_vec()).unwrap();

    // broadcast() no longer returns shards, it sends them internally
    swarms[publisher_idx].behaviour_mut().broadcast_prepared_messages(test_shards.clone()).unwrap();
    let message_root = test_shards[0].root();

    (
        message_root,
        test_shards.into_iter().map(|shard| (shard.index(), shard.shard().to_vec())).collect(),
    )
}

fn verify_received_data(
    received_messages: HashMap<(usize, MessageId), Vec<u8>>,
    received_shards: HashMap<(usize, MessageId, ShardIndex), Vec<u8>>,
    test_messages: &HashMap<MessageId, Vec<u8>>,
    test_shards: &HashMap<(usize, MessageId, ShardIndex), Vec<u8>>,
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

    for ((node_idx, message_id, index), shard) in received_shards {
        let test_shard = test_shards.get(&(publisher_idx, message_id, index)).unwrap();
        assert_eq!(
            &shard, test_shard,
            "Node {} received incorrect shard from publisher {}: message_id={}, index={}",
            node_idx, publisher_idx, message_id, index
        );
    }
}

fn assert_collection_counts(
    received_messages: &HashMap<(usize, MessageId), Vec<u8>>,
    received_shards: &HashMap<(usize, MessageId, ShardIndex), Vec<u8>>,
    expected_messages: usize,
    expected_shards: usize,
    early_stop: bool,
) {
    assert_eq!(received_messages.len(), expected_messages);
    if !early_stop {
        assert_eq!(received_shards.len(), expected_shards);
    }
}

#[allow(clippy::too_many_arguments)]
async fn broadcast_and_verify_burst(
    swarms: &mut [Swarm<Behaviour>],
    test_messages: &[Vec<u8>],
    publisher_idx: usize,
    num_nodes: usize,
    early_stop: bool,
) {
    tracing::info!("📤 Broadcasting all messages in burst mode");
    let mut test_shards = HashMap::new();
    let mut message_roots = Vec::new();
    for test_message in test_messages.iter() {
        let (message_root, shards) = broadcast_message(swarms, publisher_idx, test_message);
        message_roots.push(message_root);
        for (shard_index, shard) in shards {
            test_shards.insert((publisher_idx, message_root, shard_index), shard);
        }
    }

    tracing::info!("⏳ Collecting message events for publisher {}...", publisher_idx);
    let (received_messages, received_shards) = collect_message_events(
        swarms,
        message_roots.clone(),
        message_roots.len() * (num_nodes - 1),
        (num_nodes - 1) * test_shards.len(),
        publisher_idx,
        early_stop,
    )
    .await;

    assert_collection_counts(
        &received_messages,
        &received_shards,
        message_roots.len() * (num_nodes - 1),
        (num_nodes - 1) * test_shards.len(),
        early_stop,
    );

    verify_received_data(
        received_messages,
        received_shards,
        &test_messages
            .iter()
            .zip(message_roots)
            .map(|(message, message_root)| (message_root, message.clone()))
            .collect(),
        &test_shards,
        publisher_idx,
    );
}

async fn broadcast_and_verify_sequential(
    swarms: &mut [Swarm<Behaviour>],
    test_messages: &[Vec<u8>],
    publisher_idx: usize,
    num_nodes: usize,
    early_stop: bool,
) {
    tracing::info!("📤 Broadcasting messages sequentially with verification");

    for (i, test_message) in test_messages.iter().enumerate() {
        tracing::info!(
            "📡 Publisher {} broadcasting message {} of {} bytes (sequential mode)",
            publisher_idx,
            i,
            test_message.len()
        );

        let (message_root, test_shards) = broadcast_message(swarms, publisher_idx, test_message);

        tracing::info!("⏳ Collecting events for message {}...", i);
        let (received_messages, received_shards) = collect_message_events(
            swarms,
            vec![message_root],
            num_nodes - 1,
            (num_nodes - 1) * test_shards.len(),
            publisher_idx,
            early_stop,
        )
        .await;

        assert_collection_counts(
            &received_messages,
            &received_shards,
            num_nodes - 1,
            (num_nodes - 1) * test_shards.len(),
            early_stop,
        );

        let mut single_message = HashMap::new();
        single_message.insert(message_root, test_message.clone());

        verify_received_data(
            received_messages,
            received_shards,
            &single_message,
            &test_shards
                .iter()
                .map(|(shard_idx, data)| ((publisher_idx, message_root, *shard_idx), data.clone()))
                .collect(),
            publisher_idx,
        );

        tracing::info!("✅ Message {} verified successfully", i);
    }
}

async fn e2e(
    num_nodes: usize,
    number_of_messages: usize,
    number_of_publishers: usize,
    message_size: usize,
    early_stop: bool,
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
            broadcast_and_verify_burst(
                &mut swarms,
                &test_messages,
                publisher_idx,
                num_nodes,
                early_stop,
            )
            .await;
        } else {
            broadcast_and_verify_sequential(
                &mut swarms,
                &test_messages,
                publisher_idx,
                num_nodes,
                early_stop,
            )
            .await;
        }

        tracing::info!("✅ ✅ ✅ Publisher {} broadcast successful", publisher_idx);
    }
}

/// Initialize the tracing subscriber with error detection
fn init_tracing(env_filter: EnvFilter) {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer())
            .init();
    });
}

#[tokio::test]
async fn test_increasing_network_sizes() {
    init_tracing(
        EnvFilter::builder().with_default_directive(LevelFilter::WARN.into()).from_env_lossy(),
    );
    for num_nodes in 2..=31 {
        e2e(num_nodes, 1, 1, 1024, false, false, TransportType::Memory).await;
    }
}

#[tokio::test]
async fn random_e2e_test_memory() {
    init_tracing(
        EnvFilter::builder().with_default_directive(LevelFilter::WARN.into()).from_env_lossy(),
    );
    const NUM_TESTS: u64 = 10;
    for i in 0..NUM_TESTS {
        let seed = rand::random();
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let num_nodes = rng.gen_range(2..=100);
        let number_of_messages = rng.gen_range(1..=2);
        let number_of_publishers = rng.gen_range(1..=2);
        let message_size = rng.gen_range(256..=2048);
        let early_stop = rng.gen_bool(0.5);
        let send_in_burst = rng.gen_bool(0.5);
        println!(
            "Memory {}: Running test with seed {}: num_nodes={}, number_of_messages={}, \
             number_of_publishers={}, message_size={}, early_stop={}, send_in_burst={}",
            i,
            seed,
            num_nodes,
            number_of_messages,
            number_of_publishers,
            message_size,
            early_stop,
            send_in_burst,
        );
        e2e(
            num_nodes,
            number_of_messages,
            number_of_publishers,
            message_size,
            early_stop,
            send_in_burst,
            TransportType::Memory,
        )
        .await;
    }
}

#[tokio::test]
async fn random_e2e_test_quic() {
    init_tracing(
        EnvFilter::builder().with_default_directive(LevelFilter::WARN.into()).from_env_lossy(),
    );
    const NUM_TESTS: u64 = 10;
    for i in 0..NUM_TESTS {
        let seed = rand::random();
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let num_nodes = rng.gen_range(2..=10); // quic is slower
        let number_of_messages = rng.gen_range(1..=2);
        let number_of_publishers = rng.gen_range(1..=2);
        let message_size = rng.gen_range(256..=1024);
        let early_stop = rng.gen_bool(0.5);
        let send_in_burst = rng.gen_bool(0.5);
        println!(
            "QUIC {}: Running test with seed {}: num_nodes={}, number_of_messages={}, \
             number_of_publishers={}, message_size={}, early_stop={}, send_in_burst={}",
            i,
            seed,
            num_nodes,
            number_of_messages,
            number_of_publishers,
            message_size,
            early_stop,
            send_in_burst,
        );
        e2e(
            num_nodes,
            number_of_messages,
            number_of_publishers,
            message_size,
            early_stop,
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
    init_tracing(
        EnvFilter::builder().with_default_directive(LevelFilter::WARN.into()).from_env_lossy(),
    );

    e2e(
        num_nodes,
        1, // number_of_messages
        1, // number_of_publishers
        message_size,
        true, // early_stop
        true, // send_in_burst
        TransportType::Memory,
    )
    .await;
}
