//! Runs a node that stress tests the p2p communication of the network.
#![allow(clippy::as_conversions)]
use std::collections::HashMap;
use std::convert::Infallible;
use std::fmt::Display;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};
use std::vec;

use apollo_metrics::define_metrics;
use apollo_metrics::metrics::LossyIntoF64;
use apollo_network::network_manager::metrics::{
    BroadcastNetworkMetrics,
    GossipsubMetrics,
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
use futures::future::{select_all, BoxFuture};
use futures::{FutureExt, StreamExt};
use libp2p::gossipsub::{Sha256Topic, Topic};
use libp2p::{Multiaddr, PeerId};
use metrics_exporter_prometheus::PrometheusBuilder;
use sysinfo::{MemoryRefreshKind, Networks, ProcessRefreshKind, RefreshKind, System};
use tokio::task::JoinHandle;
use tokio::time::{interval, Duration};
use tokio_metrics::RuntimeMetricsReporterBuilder;
use tracing::{info, trace, warn, Level};

#[cfg(test)]
mod converters_test;

mod converters;
mod utils;

/// The main stress test node that manages network communication and monitoring
pub struct BroadcastNetworkStressTestNode {
    args: Args,
    network_config: NetworkConfig,
    network_manager: Option<NetworkManager>,
    broadcast_topic_client: Option<BroadcastTopicClient<StressTestMessage>>,
    broadcasted_messages_receiver: Option<BroadcastTopicServer<StressTestMessage>>,
    explore_config: Option<ExploreConfiguration>,
}

const EXPLORE_MESSAGE_SIZES_BYTES: [usize; 11] = [
    1 << 10,
    1 << 11,
    1 << 12,
    1 << 13,
    1 << 14,
    1 << 15,
    1 << 16,
    1 << 17,
    1 << 18,
    1 << 19,
    1 << 20,
];
const EXPLORE_MESSAGE_HEARTBEAT_MILLIS: [u64; 16] =
    [1, 2, 3, 4, 5, 10, 20, 30, 40, 50, 100, 150, 200, 250, 500, 1000];

lazy_static::lazy_static! {
    static ref TOPIC: Sha256Topic = Topic::new("stress_test_topic".to_string());
}

define_metrics!(
    Infra => {
        MetricGauge { BROADCAST_MESSAGE_HEARTBEAT_MILLIS, "broadcast_message_heartbeat_millis", "The number of ms we sleep between each two consecutive broadcasts" },
        MetricGauge { BROADCAST_MESSAGE_THROUGHPUT, "broadcast_message_throughput", "Throughput in bytes/second of the broadcasted " },
        MetricHistogram { BROADCAST_MESSAGE_BYTES, "broadcast_message_bytes", "Size of the stress test sent message in bytes" },
        MetricHistogram { BROADCAST_MESSAGE_SEND_DELAY_SECONDS, "broadcast_message_send_delay_seconds", "Message sending delay in seconds" },

        MetricHistogram { RECEIVE_MESSAGE_BYTES, "receive_message_bytes", "Size of the stress test received message in bytes" },
        MetricHistogram { RECEIVE_MESSAGE_DELAY_SECONDS, "receive_message_delay_seconds", "Message delay in seconds" },
        MetricHistogram { RECEIVE_MESSAGE_NEGATIVE_DELAY_SECONDS, "receive_message_negative_delay_seconds", "Negative message delay in seconds" },


        MetricGauge { NETWORK_CONNECTED_PEERS, "network_connected_peers", "Number of connected peers in the network" },
        MetricGauge { NETWORK_BLACKLISTED_PEERS, "network_blacklisted_peers", "Number of blacklisted peers in the network" },
        MetricGauge { NETWORK_ACTIVE_INBOUND_SESSIONS, "network_active_inbound_sessions", "Number of active inbound SQMR sessions" },
        MetricGauge { NETWORK_ACTIVE_OUTBOUND_SESSIONS, "network_active_outbound_sessions", "Number of active outbound SQMR sessions" },
        MetricGauge { NETWORK_GOSSIPSUB_MESH_PEERS, "network_gossipsub_mesh_peers", "Number of mesh peers" },
        MetricGauge { NETWORK_GOSSIPSUB_ALL_PEERS, "network_gossipsub_all_peers", "Total number of known peers" },
        MetricGauge { NETWORK_GOSSIPSUB_SUBSCRIBED_TOPICS, "network_gossipsub_subscribed_topics", "Number of subscribed topics" },
        MetricGauge { NETWORK_GOSSIPSUB_PROTOCOL_PEERS, "network_gossipsub_protocol_peers", "Number of gossipsub protocol peers" },
        MetricGauge { NETWORK_FLOODSUB_PROTOCOL_PEERS, "network_floodsub_protocol_peers", "Number of floodsub protocol peers" },
        MetricGauge { NETWORK_GOSSIPSUB_AVG_TOPICS_PER_PEER, "network_gossipsub_avg_topics_per_peer", "Average topics per peer" },
        MetricGauge { NETWORK_GOSSIPSUB_MAX_TOPICS_PER_PEER, "network_gossipsub_max_topics_per_peer", "Maximum topics per peer" },
        MetricGauge { NETWORK_GOSSIPSUB_MIN_TOPICS_PER_PEER, "network_gossipsub_min_topics_per_peer", "Minimum topics per peer" },
        MetricGauge { NETWORK_GOSSIPSUB_TOTAL_SUBSCRIPTIONS, "network_gossipsub_total_subscriptions", "Total topic subscriptions" },
        MetricGauge { NETWORK_GOSSIPSUB_AVG_MESH_PER_TOPIC, "network_gossipsub_avg_mesh_per_topic", "Average mesh peers per topic" },
        MetricGauge { NETWORK_GOSSIPSUB_MAX_MESH_PER_TOPIC, "network_gossipsub_max_mesh_per_topic", "Maximum mesh peers per topic" },
        MetricGauge { NETWORK_GOSSIPSUB_MIN_MESH_PER_TOPIC, "network_gossipsub_min_mesh_per_topic", "Minimum mesh peers per topic" },
        MetricGauge { NETWORK_GOSSIPSUB_POSITIVE_SCORE_PEERS, "network_gossipsub_positive_score_peers", "Peers with positive scores" },
        MetricGauge { NETWORK_GOSSIPSUB_NEGATIVE_SCORE_PEERS, "network_gossipsub_negative_score_peers", "Peers with negative scores" },
        MetricGauge { NETWORK_GOSSIPSUB_AVG_PEER_SCORE, "network_gossipsub_avg_peer_score", "Average peer score" },
        MetricCounter { NETWORK_STRESS_TEST_SENT_MESSAGES, "network_stress_test_sent_messages", "Number of stress test messages sent via broadcast", init = 0 },
        MetricCounter { NETWORK_STRESS_TEST_RECEIVED_MESSAGES, "network_stress_test_received_messages", "Number of stress test messages received via broadcast", init = 0 },
        MetricCounter { NETWORK_GOSSIPSUB_MESSAGES_RECEIVED, "network_gossipsub_messages_received", "Number of gossipsub messages received", init = 0 },
        MetricCounter { NETWORK_GOSSIPSUB_PEER_SUBSCRIBED, "network_gossipsub_peer_subscribed", "Number of peer subscriptions", init = 0 },
        MetricCounter { NETWORK_GOSSIPSUB_PEER_UNSUBSCRIBED, "network_gossipsub_peer_unsubscribed", "Number of peer unsubscriptions", init = 0 },
        MetricCounter { NETWORK_GOSSIPSUB_NOT_SUPPORTED, "network_gossipsub_not_supported", "Number of peers that don't support gossipsub", init = 0 },
        MetricCounter { NETWORK_GOSSIPSUB_SLOW_PEERS, "network_gossipsub_slow_peers", "Number of slow peers detected", init = 0 },

        MetricGauge { SYSTEM_PROCESS_CPU_USAGE_PERCENT, "system_process_cpu_usage_percent", "CPU usage percentage of the current process" },
        MetricGauge { SYSTEM_PROCESS_MEMORY_USAGE_BYTES, "system_process_memory_usage_bytes", "Memory usage in bytes of the current process" },
        MetricGauge { SYSTEM_PROCESS_VIRTUAL_MEMORY_USAGE_BYTES, "system_process_virtual_memory_usage_bytes", "Virtual memory usage in bytes of the current process" },
        MetricGauge { SYSTEM_NETWORK_BYTES_SENT_TOTAL, "system_network_bytes_sent_total", "Total bytes sent across all network interfaces since system start" },
        MetricGauge { SYSTEM_NETWORK_BYTES_RECEIVED_TOTAL, "system_network_bytes_received_total", "Total bytes received across all network interfaces since system start" },
        MetricGauge { SYSTEM_NETWORK_BYTES_SENT_CURRENT, "system_network_bytes_sent_current", "Bytes sent across all network interfaces since last measurement" },
        MetricGauge { SYSTEM_NETWORK_BYTES_RECEIVED_CURRENT, "system_network_bytes_received_current", "Bytes received across all network interfaces since last measurement" },
        MetricGauge { SYSTEM_TOTAL_MEMORY_BYTES, "system_total_memory_bytes", "Total system memory in bytes" },
        MetricGauge { SYSTEM_AVAILABLE_MEMORY_BYTES, "system_available_memory_bytes", "Available system memory in bytes" },
        MetricGauge { SYSTEM_USED_MEMORY_BYTES, "system_used_memory_bytes", "Used system memory in bytes" },
        MetricGauge { SYSTEM_CPU_COUNT, "system_cpu_count", "Number of logical CPU cores in the system" },

        MetricCounter { NETWORK_RESET_TOTAL, "network_reset_total", "Total number of network resets performed", init = 0 },
        MetricHistogram { NETWORK_RESET_DURATION_SECONDS, "network_reset_duration_seconds", "Time taken to complete network reset in seconds" }
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
    /// Only the node specified by --broadcaster-id broadcasts messages,
    /// Every 30 seconds a new combination of TPS and message size is explored
    /// Increases the throughput with each new trial.
    #[value(name = "explore")]
    Explore,
}

impl Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_possible_value().unwrap().get_name())
    }
}

#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// ID for Prometheus logging
    #[arg(short, long, env)]
    id: u64,

    /// Total number of nodes in the network
    #[arg(long, env)]
    num_nodes: u64,

    /// The port to run the Prometheus metrics server on
    #[arg(long, env)]
    metric_port: u16,

    /// The port to run the P2P network on
    #[arg(short, env, long)]
    p2p_port: u16,

    /// The addresses of the bootstrap peers (can specify multiple)
    #[arg(long, env, value_delimiter = ',')]
    bootstrap: Vec<String>,

    /// Set the verbosity level of the logger, the higher the more verbose
    #[arg(short, long, env)]
    verbosity: u8,

    /// Buffer size for the broadcast topic
    #[arg(long, env)]
    buffer_size: usize,

    /// The mode to use for the stress test.
    #[arg(long, env)]
    mode: Mode,

    /// Which node ID should do the broadcasting - for OneBroadcast and Explore modes
    #[arg(long, env, required_if_eq_any([("mode", "one"), ("mode", "explore")]))]
    broadcaster: Option<u64>,

    /// Duration each node broadcasts before switching (in seconds) - for RoundRobin mode
    #[arg(long, env, required_if_eq("mode", "rr"))]
    round_duration_seconds: Option<u64>,

    /// Size of StressTestMessage
    #[arg(long, env, required_if_eq_any([("mode", "one"), ("mode", "all"), ("mode", "rr")]))]
    message_size_bytes: Option<usize>,

    /// The time to sleep between broadcasts of StressTestMessage in milliseconds
    #[arg(long, env, required_if_eq_any([("mode", "one"), ("mode", "all"), ("mode", "rr")]))]
    heartbeat_millis: Option<u64>,

    /// Cool down duration between configuration changes in seconds - for Explore mode
    #[arg(long, env, required_if_eq("mode", "explore"))]
    explore_cool_down_duration_seconds: Option<u64>,

    /// Duration to run each configuration in seconds - for Explore mode
    #[arg(long, env, required_if_eq("mode", "explore"))]
    explore_run_duration_seconds: Option<u64>,

    /// Interval for collecting process metrics (CPU, memory) in seconds
    #[arg(long, env)]
    system_metrics_interval_seconds: u64,

    /// Minimum throughput in bytes per second - for Explore mode
    #[arg(long, env, required_if_eq("mode", "explore"))]
    explore_min_throughput_byte_per_seconds: Option<f64>,

    /// The timeout in seconds for the node.
    /// When the node runs for longer than this, it will be killed.
    #[arg(long, env)]
    timeout: u64,
}

fn get_message(id: u64, size_bytes: usize) -> StressTestMessage {
    let message = StressTestMessage::new(id, 0, vec![0; size_bytes - *METADATA_SIZE]);
    assert_eq!(Vec::<u8>::from(message.clone()).len(), size_bytes);
    message
}

/// Calculates the throughput given the message and how much to sleep between each two consecutive
/// broadcasts
fn get_throughput(message_size_bytes: usize, heartbeat_duration: Duration) -> f64 {
    let tps = Duration::from_secs(1).as_secs_f64() / heartbeat_duration.as_secs_f64();
    tps * (message_size_bytes as f64)
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ExplorePhase {
    /// In cooldown period - no broadcasting should occur
    CoolDown,
    /// In running period - broadcasting should occur (if this node is the broadcaster)
    Running,
}

#[derive(Clone)]
struct ExploreConfiguration {
    sorted_configurations: Vec<(usize, Duration)>,
    /// The broadcaster configuration index
    configuration_index: usize,
    /// Duration of the Running phase of the cycle
    run_duration_seconds: u64,
    /// Total duration for one complete cycle (cooldown + run_duration_seconds)
    cycle_duration_seconds: u64,
}

impl ExploreConfiguration {
    fn new(
        cool_down_duration_seconds: u64,
        run_duration_seconds: u64,
        min_throughput_byte_per_seconds: f64,
    ) -> ExploreConfiguration {
        let mut sorted_configurations = Vec::with_capacity(
            EXPLORE_MESSAGE_SIZES_BYTES.len() * EXPLORE_MESSAGE_HEARTBEAT_MILLIS.len(),
        );
        for message_size in EXPLORE_MESSAGE_SIZES_BYTES {
            for heartbeat_millis in EXPLORE_MESSAGE_HEARTBEAT_MILLIS {
                sorted_configurations.push((message_size, Duration::from_millis(heartbeat_millis)));
            }
        }
        sorted_configurations.retain(|(size, duration)| {
            get_throughput(*size, *duration) >= min_throughput_byte_per_seconds
        });
        sorted_configurations
            .sort_by_cached_key(|(size, duration)| get_throughput(*size, *duration) as u64);

        let cycle_duration_seconds = cool_down_duration_seconds + run_duration_seconds;

        Self {
            sorted_configurations,
            configuration_index: 0,
            run_duration_seconds,
            cycle_duration_seconds,
        }
    }

    /// Gets the current phase within the current configuration cycle
    fn get_current_phase(&self) -> ExplorePhase {
        let now_seconds = seconds_since_epoch();
        let position_in_cycle_seconds = now_seconds % self.cycle_duration_seconds;

        if position_in_cycle_seconds < self.run_duration_seconds {
            ExplorePhase::Running
        } else {
            ExplorePhase::CoolDown
        }
    }

    /// Gets the current message size and duration based on synchronized time
    fn get_current_size_and_heartbeat(&mut self) -> (usize, Duration) {
        let config_index = self.configuration_index;
        self.configuration_index += 1;
        self.sorted_configurations[config_index]
    }
}

impl BroadcastNetworkStressTestNode {
    /// Creates network configuration from arguments
    fn create_network_config(args: &Args) -> NetworkConfig {
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

        network_config
    }

    /// Creates and sets up a network manager with broadcast topic registration
    #[allow(clippy::type_complexity)]
    fn create_network_manager_with_topic(
        network_config: &NetworkConfig,
        buffer_size: usize,
    ) -> (
        NetworkManager,
        BroadcastTopicClient<StressTestMessage>,
        BroadcastTopicServer<StressTestMessage>,
    ) {
        let network_metrics = create_network_metrics();
        let mut network_manager =
            NetworkManager::new(network_config.clone(), None, Some(network_metrics));

        let peer_id_string = network_manager.get_local_peer_id();
        let peer_id = PeerId::from_str(&peer_id_string).expect("Failed to parse peer ID");
        info!("Network Manager PeerId: {peer_id}");

        let network_channels = network_manager
            .register_broadcast_topic::<StressTestMessage>(TOPIC.clone(), buffer_size, buffer_size)
            .expect("Failed to register broadcast topic");
        let BroadcastTopicChannels { broadcasted_messages_receiver, broadcast_topic_client } =
            network_channels;

        (network_manager, broadcast_topic_client, broadcasted_messages_receiver)
    }

    /// Extracts explore mode parameters from arguments with validation
    fn extract_explore_params(args: &Args) -> (u64, u64, f64) {
        let cool_down = args
            .explore_cool_down_duration_seconds
            .expect("explore_cool_down_duration_seconds required for explore mode");
        let run_duration = args
            .explore_run_duration_seconds
            .expect("explore_run_duration_seconds required for explore mode");
        let min_throughput = args
            .explore_min_throughput_byte_per_seconds
            .expect("explore_min_throughput_byte_per_seconds required for explore mode");

        (cool_down, run_duration, min_throughput)
    }

    /// Creates explore configuration and initializes message parameters
    fn setup_explore_config(args: &Args) -> Option<ExploreConfiguration> {
        if let Mode::Explore = args.mode {
            let (cool_down, run_duration, min_throughput) = Self::extract_explore_params(args);
            let explore_config = ExploreConfiguration::new(cool_down, run_duration, min_throughput);
            Some(explore_config)
        } else {
            None
        }
    }

    /// Creates a new BroadcastNetworkStressTestNode instance
    pub async fn new(args: Args) -> Self {
        // Create network configuration
        let network_config = Self::create_network_config(&args);

        // Create network manager with broadcast topic
        let (network_manager, broadcast_topic_client, broadcasted_messages_receiver) =
            Self::create_network_manager_with_topic(&network_config, args.buffer_size);

        // Setup explore configuration if needed
        let explore_config = Self::setup_explore_config(&args);

        Self {
            args,
            network_config,
            network_manager: Some(network_manager),
            broadcast_topic_client: Some(broadcast_topic_client),
            broadcasted_messages_receiver: Some(broadcasted_messages_receiver),
            explore_config,
        }
    }

    /// Starts the network manager in the background
    pub async fn start_network_manager(&mut self) -> BoxFuture<'static, ()> {
        let network_manager =
            self.network_manager.take().expect("Network manager should be available");
        async move {
            let _ = network_manager.run().await;
        }
        .boxed()
    }

    /// Recreates the network manager with fresh state
    pub async fn recreate_network_manager(&mut self) {
        // Create new network manager with broadcast topic using helper method
        let (network_manager, broadcast_topic_client, broadcasted_messages_receiver) =
            Self::create_network_manager_with_topic(&self.network_config, self.args.buffer_size);

        info!("Recreated Network Manager");

        // Update the struct with new components
        self.network_manager = Some(network_manager);
        self.broadcast_topic_client = Some(broadcast_topic_client);
        self.broadcasted_messages_receiver = Some(broadcasted_messages_receiver);
    }

    /// Gets the broadcaster ID with validation for modes that require it
    fn get_broadcaster_id(args: &Args) -> u64 {
        args.broadcaster.expect("broadcaster required for one/explore mode")
    }

    /// Determines if this node should broadcast messages based on the mode
    pub fn should_broadcast(&self) -> bool {
        match self.args.mode {
            Mode::AllBroadcast | Mode::RoundRobin => true,
            Mode::OneBroadcast | Mode::Explore => {
                let broadcaster_id = Self::get_broadcaster_id(&self.args);
                self.args.id == broadcaster_id
            }
        }
    }

    /// Starts the message sending task if this node should broadcast
    pub async fn start_message_sender(&mut self) -> Option<BoxFuture<'static, ()>> {
        if !self.should_broadcast() {
            info!("Node {} will NOT broadcast in mode `{}`", self.args.id, self.args.mode);
            return None;
        }

        info!("Node {} will broadcast in mode `{}`", self.args.id, self.args.mode);

        let broadcast_topic_client =
            self.broadcast_topic_client.take().expect("broadcast_topic_client should be available");
        let args_clone = self.args.clone();
        let explore_config = self.explore_config.clone();

        Some(
            async move {
                Self::send_stress_test_messages_impl(
                    broadcast_topic_client,
                    &args_clone,
                    &explore_config,
                )
                .await;
            }
            .boxed(),
        )
    }

    /// Implementation of the message sending logic (moved from the standalone function)
    async fn send_stress_test_messages_impl(
        mut broadcast_topic_client: BroadcastTopicClient<StressTestMessage>,
        args: &Args,
        explore_config: &Option<ExploreConfiguration>,
    ) {
        let size_bytes = args
            .message_size_bytes
            .expect("Even in explore mode message size should be set automatically.");
        let heartbeat = Duration::from_millis(
            args.heartbeat_millis
                .expect("Even in explore mode heartbeat millis should be set automatically."),
        );

        let mut message_index = 0;
        let mut message = get_message(args.id, size_bytes).clone();
        update_broadcast_metrics(message.len(), heartbeat);

        let mut interval = interval(heartbeat);
        loop {
            interval.tick().await;

            // Check if this node should broadcast based on the mode
            let should_broadcast_now = match args.mode {
                Mode::AllBroadcast | Mode::OneBroadcast => true,
                Mode::RoundRobin => should_broadcast_round_robin(args),
                Mode::Explore => {
                    explore_config
                        .as_ref()
                        .expect("ExploreConfig not available")
                        .get_current_phase()
                        == ExplorePhase::Running
                }
            };

            if should_broadcast_now {
                message.metadata.time = SystemTime::now();
                message.metadata.message_index = message_index;
                let start_time = std::time::Instant::now();
                broadcast_topic_client.broadcast_message(message.clone()).await.unwrap();
                BROADCAST_MESSAGE_SEND_DELAY_SECONDS.record(start_time.elapsed().as_secs_f64());
                BROADCAST_MESSAGE_BYTES.record(message.len() as f64);
                trace!("Node {} sent message {message_index} in mode `{}`", args.id, args.mode);
                message_index += 1;
            }
        }
    }

    /// Starts the message receiving task
    pub async fn start_message_receiver(&mut self) -> BoxFuture<'static, ()> {
        let broadcasted_messages_receiver = self
            .broadcasted_messages_receiver
            .take()
            .expect("broadcasted_messages_receiver should be available");

        async move {
            Self::receive_stress_test_messages_impl(broadcasted_messages_receiver).await;
        }
        .boxed()
    }

    /// Implementation of the message receiving logic (moved from the standalone function)
    async fn receive_stress_test_messages_impl(
        broadcasted_messages_receiver: BroadcastTopicServer<StressTestMessage>,
    ) {
        broadcasted_messages_receiver
            .for_each(|result| async {
                let (message_result, metadata) = result;
                tokio::task::spawn_blocking(|| {
                    receive_stress_test_message(message_result, metadata)
                });
            })
            .await;
    }

    /// Starts the process metrics monitoring task
    pub fn start_metrics_monitor(&self) -> BoxFuture<'static, ()> {
        let metrics_interval = self.args.system_metrics_interval_seconds;
        async move {
            monitor_process_metrics(metrics_interval).await;
        }
        .boxed()
    }

    /// Sets up and starts all tasks common to both simple and network reset modes
    async fn setup_tasks(&mut self) -> Vec<BoxFuture<'static, ()>> {
        let mut tasks = Vec::new();
        tasks.push(self.start_network_manager().await);
        tasks.push(self.start_message_receiver().await);
        tasks.push(self.start_metrics_monitor());

        if let Some(sender_task) = self.start_message_sender().await {
            tasks.push(sender_task);
        }

        tasks
    }

    /// Unified run function that handles both simple and network reset modes
    async fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        let test_timeout = Duration::from_secs(self.args.timeout);
        let start_time = tokio::time::Instant::now();

        // Main loop - restart if network reset is enabled, otherwise run once
        loop {
            if let Some(explore_config) = &mut self.explore_config {
                if self.args.id == Self::get_broadcaster_id(&self.args) {
                    BROADCAST_MESSAGE_THROUGHPUT.set(0);
                }
                while explore_config.get_current_phase() == ExplorePhase::CoolDown {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                let (size, duration) = explore_config.get_current_size_and_heartbeat();
                self.args.message_size_bytes = Some(size);
                self.args.heartbeat_millis = Some(duration.as_millis().try_into().unwrap());
            }

            info!("Starting/restarting all tasks");

            // Start all common tasks
            let mut tasks = self.setup_tasks().await;

            // Add reset coordination task only for explore mode
            if let Some(explore_config) = &self.explore_config {
                let explore_config_clone = explore_config.clone();
                assert_eq!(explore_config_clone.get_current_phase(), ExplorePhase::Running);
                let reset_task = async move {
                    while explore_config_clone.get_current_phase() == ExplorePhase::Running {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                    info!("Explore mode: CoolDown phase detected - triggering network reset");
                    NETWORK_RESET_TOTAL.increment(1);
                }
                .boxed();
                tasks.push(reset_task);
            }

            // Wait for either timeout or any task completion
            let remaining_time = test_timeout.saturating_sub(start_time.elapsed());
            let spawned_tasks: Vec<_> = tasks.into_iter().map(|task| tokio::spawn(task)).collect();
            let task_completed =
                tokio::time::timeout(remaining_time, race_and_kill_tasks(spawned_tasks))
                    .await
                    .is_ok();

            if !task_completed {
                info!("Test timeout reached");
                return Err("Test timeout".into());
            }

            // Handle task completion
            if self.explore_config.is_none() {
                return Err("Tasks should never end in simple mode".into());
            }

            // Reset mode: any task completing means restart is needed
            info!("Task completed - triggering restart");

            // Recreate network manager for clean state
            self.recreate_network_manager().await;
        }
    }
}

async fn race_and_kill_tasks(spawned_tasks: Vec<JoinHandle<()>>) {
    if spawned_tasks.is_empty() {
        return;
    }

    // Wait for any task to complete
    let (result, _index, remaining_tasks) = select_all(spawned_tasks).await;

    // Log the result of the completed task
    if let Err(e) = result {
        warn!("Task completed with error: {:?}", e);
    }

    // Abort all remaining tasks
    for task in remaining_tasks {
        task.abort();
    }
}

fn update_broadcast_metrics(message_size_bytes: usize, broadcast_heartbeat: Duration) {
    BROADCAST_MESSAGE_HEARTBEAT_MILLIS.set(broadcast_heartbeat.as_millis() as f64);
    BROADCAST_MESSAGE_THROUGHPUT.set(get_throughput(message_size_bytes, broadcast_heartbeat));
}

fn receive_stress_test_message(
    message_result: Result<StressTestMessage, Infallible>,
    _metadata: BroadcastedMessageMetadata,
) {
    let end_time = SystemTime::now();

    let received_message = message_result.unwrap();
    let start_time = received_message.metadata.time;
    let delay_seconds = match end_time.duration_since(start_time) {
        Ok(duration) => duration.as_secs_f64(),
        Err(_) => {
            let negative_duration = start_time.duration_since(end_time).unwrap();
            -negative_duration.as_secs_f64()
        }
    };

    // Use apollo_metrics for all metrics including labeled ones
    RECEIVE_MESSAGE_BYTES.record(received_message.len() as f64);

    // Use apollo_metrics histograms for latency measurements
    if delay_seconds.is_sign_positive() {
        RECEIVE_MESSAGE_DELAY_SECONDS.record(delay_seconds);
    } else {
        RECEIVE_MESSAGE_NEGATIVE_DELAY_SECONDS.record(-delay_seconds);
    }
}

fn seconds_since_epoch() -> u64 {
    let now = SystemTime::now();
    now.duration_since(UNIX_EPOCH).unwrap().as_secs()
}

fn should_broadcast_round_robin(args: &Args) -> bool {
    let now_seconds = seconds_since_epoch();
    let round_duration_seconds =
        args.round_duration_seconds.expect("round_duration_seconds required for rr mode");
    let current_round = (now_seconds / round_duration_seconds) % args.num_nodes;
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

    // Create Gossipsub metrics for monitoring gossipsub events
    let gossipsub_metrics = GossipsubMetrics {
        // Basic network topology metrics
        num_mesh_peers: NETWORK_GOSSIPSUB_MESH_PEERS,
        num_all_peers: NETWORK_GOSSIPSUB_ALL_PEERS,
        num_subscribed_topics: NETWORK_GOSSIPSUB_SUBSCRIBED_TOPICS,
        num_gossipsub_peers: NETWORK_GOSSIPSUB_PROTOCOL_PEERS,
        num_floodsub_peers: NETWORK_FLOODSUB_PROTOCOL_PEERS,

        // Topic distribution metrics
        avg_topics_per_peer: NETWORK_GOSSIPSUB_AVG_TOPICS_PER_PEER,
        max_topics_per_peer: NETWORK_GOSSIPSUB_MAX_TOPICS_PER_PEER,
        min_topics_per_peer: NETWORK_GOSSIPSUB_MIN_TOPICS_PER_PEER,
        total_topic_subscriptions: NETWORK_GOSSIPSUB_TOTAL_SUBSCRIPTIONS,

        // Mesh analysis metrics
        avg_mesh_peers_per_topic: NETWORK_GOSSIPSUB_AVG_MESH_PER_TOPIC,
        max_mesh_peers_per_topic: NETWORK_GOSSIPSUB_MAX_MESH_PER_TOPIC,
        min_mesh_peers_per_topic: NETWORK_GOSSIPSUB_MIN_MESH_PER_TOPIC,

        // Peer scoring metrics
        num_peers_with_positive_score: NETWORK_GOSSIPSUB_POSITIVE_SCORE_PEERS,
        num_peers_with_negative_score: NETWORK_GOSSIPSUB_NEGATIVE_SCORE_PEERS,
        avg_peer_score: NETWORK_GOSSIPSUB_AVG_PEER_SCORE,

        // Event-based metrics (counters)
        count_event_messages_received: NETWORK_GOSSIPSUB_MESSAGES_RECEIVED,
        count_event_peer_subscribed: NETWORK_GOSSIPSUB_PEER_SUBSCRIBED,
        count_event_peer_unsubscribed: NETWORK_GOSSIPSUB_PEER_UNSUBSCRIBED,
        count_event_gossipsub_not_supported: NETWORK_GOSSIPSUB_NOT_SUPPORTED,
        count_event_slow_peers: NETWORK_GOSSIPSUB_SLOW_PEERS,
    };

    NetworkMetrics {
        num_connected_peers: NETWORK_CONNECTED_PEERS,
        num_blacklisted_peers: NETWORK_BLACKLISTED_PEERS,
        broadcast_metrics_by_topic: Some(broadcast_metrics_by_topic),
        sqmr_metrics: Some(sqmr_metrics),
        gossipsub_metrics: Some(gossipsub_metrics),
    }
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
    let current_pid = sysinfo::get_current_pid().expect("Failed to get current process PID");

    // Initialize networks for network interface monitoring
    let mut networks = Networks::new_with_refreshed_list();

    loop {
        // system should be created and removed each loop because it affects memory usage heavily
        let system = System::new_with_specifics(
            RefreshKind::new()
                .with_processes(ProcessRefreshKind::new().with_memory().with_cpu())
                .with_memory(MemoryRefreshKind::new().with_ram()),
        );
        let total_memory: f64 = system.total_memory().into_f64();
        let available_memory: f64 = system.available_memory().into_f64();
        let used_memory: f64 = system.used_memory().into_f64();
        let cpu_count: f64 = system.cpus().len().into_f64();
        // let load_avg: f64 = system.load_average().one.into_f64();

        SYSTEM_TOTAL_MEMORY_BYTES.set(total_memory);
        SYSTEM_AVAILABLE_MEMORY_BYTES.set(available_memory);
        SYSTEM_USED_MEMORY_BYTES.set(used_memory);
        SYSTEM_CPU_COUNT.set(cpu_count);

        if let Some(process) = system.process(current_pid) {
            let cpu_usage: f64 = process.cpu_usage().into();
            let memory_usage: f64 = process.memory().into_f64();
            let virtual_memory_usage: f64 = process.virtual_memory().into_f64();

            SYSTEM_PROCESS_CPU_USAGE_PERCENT.set(cpu_usage);
            SYSTEM_PROCESS_MEMORY_USAGE_BYTES.set(memory_usage);
            SYSTEM_PROCESS_VIRTUAL_MEMORY_USAGE_BYTES.set(virtual_memory_usage);
        } else {
            warn!("Could not find process information for PID: {}", current_pid);
        }

        // Refresh network statistics and collect metrics
        networks.refresh();

        let mut total_bytes_sent: u64 = 0;
        let mut total_bytes_received: u64 = 0;
        let mut current_bytes_sent: u64 = 0;
        let mut current_bytes_received: u64 = 0;

        for (_interface_name, data) in &networks {
            total_bytes_sent += data.total_transmitted();
            total_bytes_received += data.total_received();
            current_bytes_sent += data.transmitted();
            current_bytes_received += data.received();
        }

        SYSTEM_NETWORK_BYTES_SENT_TOTAL.set(total_bytes_sent.into_f64());
        SYSTEM_NETWORK_BYTES_RECEIVED_TOTAL.set(total_bytes_received.into_f64());
        SYSTEM_NETWORK_BYTES_SENT_CURRENT.set(current_bytes_sent.into_f64());
        SYSTEM_NETWORK_BYTES_RECEIVED_CURRENT.set(current_bytes_received.into_f64());

        tokio::time::sleep(duration).await;
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
        args.message_size_bytes.unwrap_or(*METADATA_SIZE) >= *METADATA_SIZE,
        "Message size must be at least {} bytes",
        *METADATA_SIZE
    );

    // Set up metrics
    let builder = PrometheusBuilder::new().with_http_listener(SocketAddr::V4(SocketAddrV4::new(
        Ipv4Addr::UNSPECIFIED,
        args.metric_port,
    )));

    builder.install().expect("Failed to install prometheus recorder/exporter");

    // Start the tokio runtime metrics reporter to automatically collect and export runtime metrics
    tokio::spawn(
        RuntimeMetricsReporterBuilder::default()
            .with_interval(Duration::from_secs(args.system_metrics_interval_seconds))
            .describe_and_run(),
    );

    // Create and run the stress test node
    let stress_test_node = BroadcastNetworkStressTestNode::new(args).await;
    stress_test_node.run().await
}
