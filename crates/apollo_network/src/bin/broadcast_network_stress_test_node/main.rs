//! Runs a node that stress tests the p2p communication of the network.

use std::convert::Infallible;
use std::fmt::Display;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};
use std::vec;

use apollo_network::network_manager::{
    BroadcastTopicChannels,
    BroadcastTopicClient,
    BroadcastTopicClientTrait,
    BroadcastTopicServer,
    NetworkManager,
};
use apollo_network::NetworkConfig;
use apollo_network_types::network_types::BroadcastedMessageMetadata;
use clap::{Parser, ValueEnum};
use converters::{StressTestMessage, METADATA_SIZE};
use futures::future::join_all;
use futures::StreamExt;
use libp2p::gossipsub::{Sha256Topic, Topic};
use libp2p::{Multiaddr, PeerId};
use metrics::{counter, histogram};
use metrics_exporter_prometheus::PrometheusBuilder;
use tokio::sync::Mutex;
use tokio::time::Duration;
use tracing::{info, trace, Level};

#[cfg(test)]
mod converters_test;

mod converters;
mod utils;

/// Maximum number of nodes supported - adjust as needed
const MAX_NODES: usize = 1 << 9;

lazy_static::lazy_static! {
    static ref TOPIC: Sha256Topic = Topic::new("stress_test_topic".to_string());

    static ref MAX_MESSAGE_INDICES: Vec<Mutex<Option<u64>>> = (0..MAX_NODES).map(|_| Mutex::new(None)).collect();
}

#[derive(Debug, Clone, ValueEnum)]
enum Mode {
    /// All nodes broadcast messages
    #[value(name = "all")]
    AllBroadcast,
    /// Only the node specified by --broadcaster-id broadcasts messages
    #[value(name = "one")]
    OneBroadcast,
    /// Nodes take turns broadcasting in round-robin fashion
    #[value(name = "rr")]
    RoundRobin,
}

impl Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_possible_value().unwrap().get_name())
    }
}

#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
struct Args {
    /// ID for Prometheus logging
    #[arg(short, long, env)]
    id: u64,

    /// Total number of nodes in the network - for RoundRobin mode
    #[arg(long, env, default_value_t = 3)]
    num_nodes: u64,

    /// The port to run the Prometheus metrics server on
    #[arg(long, env, default_value_t = 2000)]
    metric_port: u16,

    /// The port to run the P2P network on
    #[arg(short, env, long, default_value_t = 10000)]
    p2p_port: u16,

    /// The addresses of the bootstrap peers (can specify multiple)
    #[arg(long, env, value_delimiter = ',')]
    bootstrap: Vec<String>,

    /// Set the verbosity level of the logger, the higher the more verbose
    #[arg(short, long, env, default_value_t = 0)]
    verbosity: u8,

    /// Buffer size for the broadcast topic
    // Default from crates/apollo_consensus_manager/src/config.rs
    #[arg(short, long, env, default_value_t = 10000)]
    buffer_size: usize,

    /// Size of StressTestMessage
    #[arg(short, long, env, default_value_t = 1 << 10)]
    message_size_bytes: usize,

    /// The time to sleep between broadcasts of StressTestMessage in milliseconds
    #[arg(long, env, default_value_t = 1)]
    heartbeat_millis: u64,

    /// The mode to use for the stress test.
    #[arg(long, env, default_value = "all")]
    mode: Mode,

    /// Which node ID should do the broadcasting - for OneBroadcast mode
    #[arg(long, env, default_value_t = 1)]
    broadcaster: u64,

    /// Duration each node broadcasts before switching (in seconds) - for RoundRobin mode
    #[arg(long, env, default_value_t = 3)]
    round_duration_seconds: u64,
}

async fn send_stress_test_messages(
    mut broadcast_topic_client: BroadcastTopicClient<StressTestMessage>,
    args: &Args,
) {
    let mut message =
        StressTestMessage::new(args.id, 0, vec![0; args.message_size_bytes - *METADATA_SIZE]);
    let duration = Duration::from_millis(args.heartbeat_millis);

    let mut message_index = 0;
    loop {
        // Check if this node should broadcast based on the mode
        let should_broadcast_now = match args.mode {
            Mode::AllBroadcast => true,
            Mode::OneBroadcast => {
                unreachable!("When OneBroadcast mode is used, this function should not be called")
            }
            Mode::RoundRobin => should_broadcast_round_robin(args),
        };

        if should_broadcast_now {
            message.metadata.time = SystemTime::now();
            message.metadata.message_index = message_index;
            broadcast_topic_client.broadcast_message(message.clone()).await.unwrap();
            trace!("Node {} sent message {message_index} in mode `{}`", args.id, args.mode);
            counter!("messages_sent_total").increment(1);
            message_index += 1;
        }

        tokio::time::sleep(duration).await;
    }
}

fn receive_stress_test_message(
    message_result: Result<StressTestMessage, Infallible>,
    _metadata: BroadcastedMessageMetadata,
) {
    let end_time = SystemTime::now();

    let received_message = message_result.unwrap();
    let sender_id = received_message.metadata.sender_id;
    let start_time = received_message.metadata.time;
    let delay_seconds = match end_time.duration_since(start_time) {
        Ok(duration) => duration.as_secs_f64(),
        Err(_) => {
            let negative_duration = start_time.duration_since(end_time).unwrap();
            -negative_duration.as_secs_f64()
        }
    };

    // let delay_micros = duration.as_micros().try_into().unwrap();
    let current_message_index = received_message.metadata.message_index;

    // Use proper metrics with labels instead of dynamic metric names
    counter!("messages_received_total").increment(1);
    counter!("messages_received_by_sender_total", "sender_id" => sender_id.to_string())
        .increment(1);

    // Use histogram for latency measurements
    if delay_seconds >= 0.0 {
        histogram!("message_delay_seconds").record(delay_seconds);
        histogram!("message_delay_by_sender_seconds", "sender_id" => sender_id.to_string())
            .record(delay_seconds);
    } else {
        histogram!("message_negative_delay_seconds").record(-delay_seconds);
        histogram!("message_negative_delay_by_sender_seconds", "sender_id" => sender_id.to_string())
        .record(-delay_seconds);
    }

    counter!("bytes_received_total").increment(received_message.byte_size().try_into().unwrap());

    // Handle message ordering and update last message indices
    handle_message_ordering(sender_id, current_message_index);
}

/// Helper function to handle message ordering tracking and out-of-order detection
fn handle_message_ordering(sender_id: u64, current_message_index: u64) {
    let sender_index: usize = sender_id.try_into().unwrap();
    if sender_index >= MAX_NODES {
        panic!("Received message from sender_id {sender_id} which exceeds MAX_NODES {MAX_NODES}");
    }

    let mut max_index_guard = MAX_MESSAGE_INDICES[sender_index].blocking_lock();
    let Some(max_index) = *max_index_guard else {
        *max_index_guard = Some(current_message_index);
        return;
    };

    // update max value
    *max_index_guard = Some(current_message_index.max(max_index));
    let expected_index = max_index + 1;

    if current_message_index == expected_index {
        return;
    }

    // Use labels instead of dynamic metric names
    counter!("messages_out_of_order_total").increment(1);
    counter!("messages_out_of_order_by_sender_total", "sender_id" => sender_id.to_string())
        .increment(1);

    if expected_index < current_message_index {
        let missed_messages = current_message_index - expected_index;
        counter!("messages_missing_total").increment(missed_messages);
        counter!("messages_missing_by_sender_total", "sender_id" => sender_id.to_string())
            .increment(missed_messages);
        return;
    }

    if max_index == current_message_index {
        counter!("messages_duplicate_total").increment(1);
        counter!("messages_duplicate_by_sender_total", "sender_id" => sender_id.to_string())
            .increment(1);
        // TODO(AndrewL): should this ever happen? does libp2p prevent this?
        // Note: this count does not account fot all duplicates...
        return;
    }

    if current_message_index < max_index {
        counter!("messages_missing_retrieved_total").increment(1);
        counter!("messages_missing_retrieved_by_sender_total", "sender_id" => sender_id.to_string())
            .increment(1);
    }
}

fn should_broadcast_round_robin(args: &Args) -> bool {
    let now = SystemTime::now();
    let now_seconds = now.duration_since(UNIX_EPOCH).unwrap().as_secs();
    let current_round = (now_seconds / args.round_duration_seconds) % args.num_nodes;
    args.id == current_round
}

async fn receive_stress_test_messages(
    broadcasted_messages_receiver: BroadcastTopicServer<StressTestMessage>,
) {
    broadcasted_messages_receiver
        .for_each(|result| async {
            let (message_result, metadata) = result;
            tokio::task::spawn_blocking(|| receive_stress_test_message(message_result, metadata));
        })
        .await;
    unreachable!("BroadcastTopicServer stream should never terminate...");
}

fn create_peer_private_key(peer_index: u64) -> [u8; 32] {
    let array = peer_index.to_le_bytes();
    assert_eq!(array.len(), 8);
    let mut private_key = [0u8; 32];
    private_key[0..8].copy_from_slice(&array);
    private_key
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let level = match args.verbosity {
        0 => None,
        1 => Some(Level::ERROR),
        2 => Some(Level::WARN),
        3 => Some(Level::INFO),
        4 => Some(Level::DEBUG),
        _ => Some(Level::TRACE),
    };
    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder().with_max_level(level).finish(),
    )
    .expect("Failed to set global default subscriber");

    println!("Starting network stress test with args:\n{args:?}");

    assert!(
        args.message_size_bytes >= *METADATA_SIZE,
        "Message size must be at least {} bytes",
        *METADATA_SIZE
    );
    assert!(
        args.num_nodes <= MAX_NODES.try_into().unwrap(),
        "num_nodes must be less than or equal to {MAX_NODES}"
    );

    let builder = PrometheusBuilder::new().with_http_listener(SocketAddr::V4(SocketAddrV4::new(
        Ipv4Addr::UNSPECIFIED,
        args.metric_port,
    )));

    builder.install().expect("Failed to install prometheus recorder/exporter");

    let peer_private_key = create_peer_private_key(args.id);
    let peer_private_key_hex =
        peer_private_key.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
    info!("Secret Key: {peer_private_key_hex:#?}");

    let mut network_config = NetworkConfig {
        port: args.p2p_port,
        secret_key: Some(peer_private_key.to_vec()),
        ..Default::default()
    };
    if !args.bootstrap.is_empty() {
        let bootstrap_peers: Vec<Multiaddr> =
            args.bootstrap.iter().map(|s| Multiaddr::from_str(s.trim()).unwrap()).collect();
        network_config.bootstrap_peer_multiaddr = Some(bootstrap_peers);
    }

    let mut network_manager = NetworkManager::new(network_config, None, None);

    let peer_id_string = network_manager.get_local_peer_id();
    let peer_id = PeerId::from_str(&peer_id_string).unwrap();
    info!("My PeerId: {peer_id}");

    let network_channels = network_manager
        .register_broadcast_topic::<StressTestMessage>(TOPIC.clone(), args.buffer_size)
        .unwrap();
    let BroadcastTopicChannels { broadcasted_messages_receiver, broadcast_topic_client } =
        network_channels;

    let mut tasks = Vec::new();

    tasks.push(tokio::spawn(async move {
        // Start the network manager to handle incoming connections and messages.
        network_manager.run().await.unwrap();
        unreachable!("Network manager should not exit");
    }));

    tasks.push(tokio::spawn(async move {
        receive_stress_test_messages(broadcasted_messages_receiver).await;
        unreachable!("Broadcast topic receiver should not exit");
    }));

    // Check if this node should broadcast based on the mode
    let should_broadcast = match args.mode {
        Mode::AllBroadcast | Mode::RoundRobin => true,
        Mode::OneBroadcast => args.id == args.broadcaster,
    };

    if should_broadcast {
        info!("Node {} will broadcast in mode `{}`", args.id, args.mode);
        let args_clone = args.clone();
        tasks.push(tokio::spawn(async move {
            send_stress_test_messages(broadcast_topic_client, &args_clone).await;
            unreachable!("Broadcast topic client should not exit");
        }));
    } else {
        info!("Node {} will NOT broadcast in mode `{}`", args.id, args.mode);
    }

    join_all(tasks.into_iter()).await;
}
