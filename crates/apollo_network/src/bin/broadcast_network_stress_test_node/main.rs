//! Runs a node that stress tests the p2p communication of the network.

use std::collections::HashMap;
use std::convert::Infallible;
use std::fmt::Display;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};
use std::vec;

use apollo_metrics::define_metrics;
use apollo_network::network_manager::metrics::{
    BroadcastNetworkMetrics,
    NetworkMetrics,
    SqmrNetworkMetrics,
};
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
use metrics_exporter_prometheus::PrometheusBuilder;
use sysinfo::{Process, System};
use tokio::sync::Mutex;
use tokio::time::Duration;
use tracing::{info, trace, warn, Level};

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

// Label definitions for sender_id metrics
pub const LABEL_NAME_SENDER_ID: &str = "sender_id";

// Label definitions for node identification
pub const LABEL_NAME_HOSTNAME: &str = "hostname";
pub const LABEL_NAME_TOTAL_MEMORY: &str = "total_memory_gb";
pub const LABEL_NAME_CPU_COUNT: &str = "cpu_count";

// For dynamic labels like sender_id, we define empty label permutations
// and use dynamic labels at runtime
const EMPTY_LABELS: &[&[(&str, &str)]] = &[];

define_metrics!(
    Infra => {
        // Network peer connection metrics
        MetricGauge { NETWORK_CONNECTED_PEERS, "network_connected_peers", "Number of connected peers in the network" },
        MetricGauge { NETWORK_BLACKLISTED_PEERS, "network_blacklisted_peers", "Number of blacklisted peers in the network" },

        // Stress test broadcast metrics
        MetricCounter { NETWORK_STRESS_TEST_SENT_MESSAGES, "network_stress_test_sent_messages", "Number of stress test messages sent via broadcast", init = 0 },
        MetricCounter { NETWORK_STRESS_TEST_RECEIVED_MESSAGES, "network_stress_test_received_messages", "Number of stress test messages received via broadcast", init = 0 },

        // SQMR session metrics
        MetricGauge { NETWORK_ACTIVE_INBOUND_SESSIONS, "network_active_inbound_sessions", "Number of active inbound SQMR sessions" },
        MetricGauge { NETWORK_ACTIVE_OUTBOUND_SESSIONS, "network_active_outbound_sessions", "Number of active outbound SQMR sessions" },

        // Stress test metrics - regular counters
        MetricCounter { MESSAGES_SENT_TOTAL, "messages_sent_total", "Total number of messages sent", init = 0 },
        MetricCounter { MESSAGES_RECEIVED_TOTAL, "messages_received_total", "Total number of messages received", init = 0 },
        MetricCounter { BYTES_RECEIVED_TOTAL, "bytes_received_total", "Total bytes received", init = 0 },
        MetricCounter { MESSAGES_OUT_OF_ORDER_TOTAL, "messages_out_of_order_total", "Total out-of-order messages", init = 0 },
        MetricCounter { MESSAGES_MISSING_TOTAL, "messages_missing_total", "Total missing messages", init = 0 },
        MetricCounter { MESSAGES_DUPLICATE_TOTAL, "messages_duplicate_total", "Total duplicate messages", init = 0 },
        MetricCounter { MESSAGES_MISSING_RETRIEVED_TOTAL, "messages_missing_retrieved_total", "Total missing messages that were later retrieved", init = 0 },

        // Stress test histograms - regular histograms
        MetricHistogram { MESSAGE_DELAY_SECONDS, "message_delay_seconds", "Message delay in seconds" },
        MetricHistogram { MESSAGE_NEGATIVE_DELAY_SECONDS, "message_negative_delay_seconds", "Negative message delay in seconds" },

        // Stress test metrics with sender_id labels - using empty label arrays for dynamic labels
        LabeledMetricCounter { MESSAGES_RECEIVED_BY_SENDER_TOTAL, "messages_received_by_sender_total", "Total messages received by sender", init = 0, labels = EMPTY_LABELS },
        LabeledMetricCounter { MESSAGES_OUT_OF_ORDER_BY_SENDER_TOTAL, "messages_out_of_order_by_sender_total", "Total out-of-order messages by sender", init = 0, labels = EMPTY_LABELS },
        LabeledMetricCounter { MESSAGES_MISSING_BY_SENDER_TOTAL, "messages_missing_by_sender_total", "Total missing messages by sender", init = 0, labels = EMPTY_LABELS },
        LabeledMetricCounter { MESSAGES_DUPLICATE_BY_SENDER_TOTAL, "messages_duplicate_by_sender_total", "Total duplicate messages by sender", init = 0, labels = EMPTY_LABELS },
        LabeledMetricCounter { MESSAGES_MISSING_RETRIEVED_BY_SENDER_TOTAL, "messages_missing_retrieved_by_sender_total", "Total missing messages later retrieved by sender", init = 0, labels = EMPTY_LABELS },

        // Stress test histograms with sender_id labels
        LabeledMetricHistogram { MESSAGE_DELAY_BY_SENDER_SECONDS, "message_delay_by_sender_seconds", "Message delay in seconds by sender", labels = EMPTY_LABELS },
        LabeledMetricHistogram { MESSAGE_NEGATIVE_DELAY_BY_SENDER_SECONDS, "message_negative_delay_by_sender_seconds", "Negative message delay in seconds by sender", labels = EMPTY_LABELS },

        // Process metrics
        MetricGauge { PROCESS_CPU_USAGE_PERCENT, "process_cpu_usage_percent", "CPU usage percentage of the current process" },
        MetricGauge { PROCESS_MEMORY_USAGE_BYTES, "process_memory_usage_bytes", "Memory usage in bytes of the current process" },
        MetricGauge { PROCESS_VIRTUAL_MEMORY_USAGE_BYTES, "process_virtual_memory_usage_bytes", "Virtual memory usage in bytes of the current process" },
        // MetricGauge { PROCESS_THREAD_COUNT, "process_thread_count", "Number of threads in the current process" },
        MetricGauge { PROCESS_UPTIME_SECONDS, "process_uptime_seconds", "Process uptime in seconds" },

        // System-level metrics to distinguish between physical nodes
        // Use these metrics to determine if processes are running on different physical nodes:
        // - Different hostnames indicate different nodes
        // - Different system uptime suggests different nodes (unless containers restarted simultaneously)
        // - Different total memory/CPU counts indicate different hardware configurations
        // - Different load averages suggest independent workloads on separate nodes
        // MetricGauge { SYSTEM_UPTIME_SECONDS, "system_uptime_seconds", "System uptime in seconds" },
        MetricGauge { SYSTEM_TOTAL_MEMORY_BYTES, "system_total_memory_bytes", "Total system memory in bytes" },
        MetricGauge { SYSTEM_AVAILABLE_MEMORY_BYTES, "system_available_memory_bytes", "Available system memory in bytes" },
        MetricGauge { SYSTEM_USED_MEMORY_BYTES, "system_used_memory_bytes", "Used system memory in bytes" },
        MetricGauge { SYSTEM_CPU_COUNT, "system_cpu_count", "Number of logical CPU cores in the system" },
        MetricGauge { SYSTEM_LOAD_AVERAGE_1MIN, "system_load_average_1min", "System load average over 1 minute" },
        // MetricGauge { SYSTEM_IDENTIFIER, "system_identifier", "A sum of many system properties to help with distinguishing if two containers are running on different nodes" },
        // MetricGauge { SYSTEM_LOAD_AVERAGE_5MIN, "system_load_average_5min", "System load average over 5 minutes" },
        // MetricGauge { SYSTEM_LOAD_AVERAGE_15MIN, "system_load_average_15min", "System load average over 15 minutes" },

        // Node identification metric with hostname label
        // LabeledMetricGauge { NODE_INFO, "node_info", "Node identification information with hostname and static system info", labels = EMPTY_LABELS },
    },
);

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

    /// Interval for collecting process metrics (CPU, memory) in seconds
    #[arg(long, env, default_value_t = 10)]
    system_metrics_interval_seconds: u64,
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
            Mode::AllBroadcast | Mode::OneBroadcast => true,
            Mode::RoundRobin => should_broadcast_round_robin(args),
        };

        if should_broadcast_now {
            message.metadata.time = SystemTime::now();
            message.metadata.message_index = message_index;
            broadcast_topic_client.broadcast_message(message.clone()).await.unwrap();
            trace!("Node {} sent message {message_index} in mode `{}`", args.id, args.mode);
            MESSAGES_SENT_TOTAL.increment(1);
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

    // Use apollo_metrics for all metrics including labeled ones
    MESSAGES_RECEIVED_TOTAL.increment(1);
    MESSAGES_RECEIVED_BY_SENDER_TOTAL
        .increment_dynamic(1, &[(LABEL_NAME_SENDER_ID, sender_id.to_string())]);

    // Use apollo_metrics histograms for latency measurements
    if delay_seconds.is_sign_positive() {
        MESSAGE_DELAY_SECONDS.record(delay_seconds);
        MESSAGE_DELAY_BY_SENDER_SECONDS
            .record_dynamic(delay_seconds, &[(LABEL_NAME_SENDER_ID, sender_id.to_string())]);
    } else {
        MESSAGE_NEGATIVE_DELAY_SECONDS.record(-delay_seconds);
        MESSAGE_NEGATIVE_DELAY_BY_SENDER_SECONDS
            .record_dynamic(-delay_seconds, &[(LABEL_NAME_SENDER_ID, sender_id.to_string())]);
    }

    BYTES_RECEIVED_TOTAL.increment(received_message.byte_size().try_into().unwrap());

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

    // Use apollo_metrics for all metrics including labeled ones
    MESSAGES_OUT_OF_ORDER_TOTAL.increment(1);
    MESSAGES_OUT_OF_ORDER_BY_SENDER_TOTAL
        .increment_dynamic(1, &[(LABEL_NAME_SENDER_ID, sender_id.to_string())]);

    if expected_index < current_message_index {
        let missed_messages = current_message_index - expected_index;
        MESSAGES_MISSING_TOTAL.increment(missed_messages);
        MESSAGES_MISSING_BY_SENDER_TOTAL
            .increment_dynamic(missed_messages, &[(LABEL_NAME_SENDER_ID, sender_id.to_string())]);
        return;
    }

    if max_index == current_message_index {
        MESSAGES_DUPLICATE_TOTAL.increment(1);
        MESSAGES_DUPLICATE_BY_SENDER_TOTAL
            .increment_dynamic(1, &[(LABEL_NAME_SENDER_ID, sender_id.to_string())]);
        // TODO(AndrewL): should this ever happen? does libp2p prevent this?
        // Note: this count does not account fot all duplicates...
        return;
    }

    if current_message_index < max_index {
        MESSAGES_MISSING_RETRIEVED_TOTAL.increment(1);
        MESSAGES_MISSING_RETRIEVED_BY_SENDER_TOTAL
            .increment_dynamic(1, &[(LABEL_NAME_SENDER_ID, sender_id.to_string())]);
    }
}

fn should_broadcast_round_robin(args: &Args) -> bool {
    let now = SystemTime::now();
    let now_seconds = now.duration_since(UNIX_EPOCH).unwrap().as_secs();
    let current_round = (now_seconds / args.round_duration_seconds) % args.num_nodes;
    args.id == current_round
}

/// Creates comprehensive network metrics for monitoring the stress test network performance.
/// Uses the lazy static metrics defined above.
fn create_network_metrics() -> NetworkMetrics {
    // Create broadcast metrics for the stress test topic
    let stress_test_broadcast_metrics = BroadcastNetworkMetrics {
        num_sent_broadcast_messages: NETWORK_STRESS_TEST_SENT_MESSAGES,
        num_received_broadcast_messages: NETWORK_STRESS_TEST_RECEIVED_MESSAGES,
    };

    // Create a map with broadcast metrics for our stress test topic
    let mut broadcast_metrics_by_topic = HashMap::new();
    broadcast_metrics_by_topic.insert(TOPIC.hash(), stress_test_broadcast_metrics);

    // Create SQMR metrics for session monitoring
    let sqmr_metrics = SqmrNetworkMetrics {
        num_active_inbound_sessions: NETWORK_ACTIVE_INBOUND_SESSIONS,
        num_active_outbound_sessions: NETWORK_ACTIVE_OUTBOUND_SESSIONS,
    };

    NetworkMetrics {
        num_connected_peers: NETWORK_CONNECTED_PEERS,
        num_blacklisted_peers: NETWORK_BLACKLISTED_PEERS,
        broadcast_metrics_by_topic: Some(broadcast_metrics_by_topic),
        sqmr_metrics: Some(sqmr_metrics),
    }
}

// Registers all the stress test metrics.
// fn register_metrics() {
//     // Network metrics
//     NETWORK_CONNECTED_PEERS.register();
//     NETWORK_BLACKLISTED_PEERS.register();
//     NETWORK_STRESS_TEST_SENT_MESSAGES.register();
//     NETWORK_STRESS_TEST_RECEIVED_MESSAGES.register();
//     NETWORK_ACTIVE_INBOUND_SESSIONS.register();
//     NETWORK_ACTIVE_OUTBOUND_SESSIONS.register();

//     // Stress test metrics
//     MESSAGES_SENT_TOTAL.register();
//     MESSAGES_RECEIVED_TOTAL.register();
//     BYTES_RECEIVED_TOTAL.register();
//     MESSAGES_OUT_OF_ORDER_TOTAL.register();
//     MESSAGES_MISSING_TOTAL.register();
//     MESSAGES_DUPLICATE_TOTAL.register();
//     MESSAGES_MISSING_RETRIEVED_TOTAL.register();
//     MESSAGE_DELAY_SECONDS.register();
//     MESSAGE_NEGATIVE_DELAY_SECONDS.register();

//     // Labeled stress test metrics
//     MESSAGES_RECEIVED_BY_SENDER_TOTAL.register();
//     MESSAGES_OUT_OF_ORDER_BY_SENDER_TOTAL.register();
//     MESSAGES_MISSING_BY_SENDER_TOTAL.register();
//     MESSAGES_DUPLICATE_BY_SENDER_TOTAL.register();
//     MESSAGES_MISSING_RETRIEVED_BY_SENDER_TOTAL.register();
//     MESSAGE_DELAY_BY_SENDER_SECONDS.register();
//     MESSAGE_NEGATIVE_DELAY_BY_SENDER_SECONDS.register();
// }

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

async fn monitor_process_metrics(interval_seconds: u64) {
    let duration = Duration::from_secs(interval_seconds);
    let mut system = System::new_all();
    let current_pid = sysinfo::get_current_pid().expect("Failed to get current process PID");
    let process_start_time = SystemTime::now();

    loop {
        // Refresh system information
        system.refresh_all();

        SYSTEM_TOTAL_MEMORY_BYTES.set(system.total_memory() as u32);
        SYSTEM_AVAILABLE_MEMORY_BYTES.set(system.available_memory() as f64);
        SYSTEM_USED_MEMORY_BYTES.set(system.used_memory() as f64);
        SYSTEM_CPU_COUNT.set(system.cpus().len() as f64);

        // SYSTEM_IDENTIFIER.set(system.total_memory() + system.available_memory() +
        // system.used_memory());

        // Update load averages
        let load_avg = sysinfo::System::load_average();
        SYSTEM_LOAD_AVERAGE_1MIN.set(load_avg.one);
        // SYSTEM_LOAD_AVERAGE_5MIN.set(load_avg.five);
        // SYSTEM_LOAD_AVERAGE_15MIN.set(load_avg.fifteen);

        // Update process-specific metrics
        if let Some(process) = system.process(current_pid) {
            // Update CPU usage percentage
            PROCESS_CPU_USAGE_PERCENT.set(process.cpu_usage() as f64);

            // Update memory usage in bytes
            PROCESS_MEMORY_USAGE_BYTES.set(process.memory() as f64);

            // Update virtual memory usage in bytes
            PROCESS_VIRTUAL_MEMORY_USAGE_BYTES.set(process.virtual_memory() as f64);

            // Update process uptime in seconds
            let uptime = process_start_time.elapsed().unwrap_or_default().as_secs_f64();
            PROCESS_UPTIME_SECONDS.set(uptime);
        } else {
            warn!("Could not find process information for PID: {}", current_pid);
        }

        tokio::time::sleep(duration).await;
    }
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

    // Register all metrics before creating the network metrics
    // register_metrics();

    // Create comprehensive network metrics for stress test monitoring
    let network_metrics = create_network_metrics();

    let mut network_manager = NetworkManager::new(network_config, None, Some(network_metrics));

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

    // Add process metrics monitoring task
    let metrics_interval = args.system_metrics_interval_seconds;
    tasks.push(tokio::spawn(async move {
        monitor_process_metrics(metrics_interval).await;
        unreachable!("Process metrics monitor should not exit");
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
