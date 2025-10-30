use std::str::FromStr;
use std::time::{Duration, SystemTime};

use apollo_network::network_manager::NetworkManager;
use apollo_network::NetworkConfig;
use futures::future::{select_all, BoxFuture};
use futures::FutureExt;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::{Multiaddr, PeerId};
use tokio::task::JoinHandle;
use tokio::time::interval;
use tracing::{info, trace, warn};

use crate::args::{Args, Mode, NetworkProtocol};
use crate::converters::{StressTestMessage, METADATA_SIZE};
use crate::explore_config::{extract_explore_params, ExploreConfiguration, ExplorePhase};
use crate::message_handling::{MessageReceiver, MessageSender};
use crate::metrics::{
    monitor_process_metrics,
    receive_stress_test_message,
    seconds_since_epoch,
    update_broadcast_metrics,
    BROADCAST_MESSAGE_BYTES,
    BROADCAST_MESSAGE_BYTES_SUM,
    BROADCAST_MESSAGE_COUNT,
    BROADCAST_MESSAGE_SEND_DELAY_SECONDS,
    BROADCAST_MESSAGE_THROUGHPUT,
    NETWORK_RESET_TOTAL,
};
use crate::network_channels::{create_network_manager_with_channels, NetworkChannels};

/// The main stress test node that manages network communication and monitoring
pub struct BroadcastNetworkStressTestNode {
    args: Args,
    network_config: NetworkConfig,
    network_manager: Option<NetworkManager>,
    network_channels: NetworkChannels,
    explore_config: Option<ExploreConfiguration>,
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

        network_config.discovery_config.heartbeat_interval = Duration::from_secs(99999999);

        if !args.bootstrap.is_empty() {
            let bootstrap_peers: Vec<Multiaddr> =
                args.bootstrap.iter().map(|s| Multiaddr::from_str(s.trim()).unwrap()).collect();
            network_config.bootstrap_peer_multiaddr = Some(bootstrap_peers);
        }

        network_config
    }

    /// Creates explore configuration and initializes message parameters
    fn setup_explore_config(args: &Args) -> Option<ExploreConfiguration> {
        if let Mode::Explore = args.mode {
            let (cool_down, run_duration, min_throughput, min_message_size) =
                extract_explore_params(args);
            let explore_config = ExploreConfiguration::new(
                cool_down,
                run_duration,
                min_throughput,
                min_message_size,
            );
            Some(explore_config)
        } else {
            None
        }
    }

    /// Creates a new BroadcastNetworkStressTestNode instance
    pub async fn new(args: Args) -> Self {
        // Create network configuration
        let network_config = Self::create_network_config(&args);

        // Create network manager with protocol channels
        let (network_manager, network_channels) = create_network_manager_with_channels(
            &network_config,
            args.buffer_size,
            &args.network_protocol,
            args.num_nodes,
            args.id,
            &args.bootstrap,
        );

        // Setup explore configuration if needed
        let explore_config = Self::setup_explore_config(&args);

        Self {
            args,
            network_config,
            network_manager: Some(network_manager),
            network_channels,
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
        // Create new network manager with protocol channels using helper method
        let (network_manager, network_channels) = create_network_manager_with_channels(
            &self.network_config,
            self.args.buffer_size,
            &self.args.network_protocol,
            self.args.num_nodes,
            self.args.id,
            &self.args.bootstrap,
        );

        info!("Recreated Network Manager");

        // Update the struct with new components
        self.network_manager = Some(network_manager);
        self.network_channels = network_channels;
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

    fn get_peers(&self) -> Vec<PeerId> {
        self.network_config
            .bootstrap_peer_multiaddr
            .as_ref()
            .map(|peers| {
                peers.iter().map(|m| DialOpts::from(m.clone()).get_peer_id().unwrap()).collect()
            })
            .unwrap_or_default()
    }

    /// Starts the message sending task if this node should broadcast
    pub async fn start_message_sender(&mut self) -> Option<BoxFuture<'static, ()>> {
        if !self.should_broadcast() {
            info!("Node {} will NOT broadcast in mode `{}`", self.args.id, self.args.mode);
            return None;
        }

        info!("Node {} will broadcast in mode `{}`", self.args.id, self.args.mode);

        let message_sender = self.network_channels.take_sender();
        let args_clone = self.args.clone();
        let explore_config = self.explore_config.clone();
        let peers = self.get_peers();

        Some(
            async move {
                Self::send_stress_test_messages_impl(
                    message_sender,
                    &args_clone,
                    peers,
                    &explore_config,
                )
                .await;
            }
            .boxed(),
        )
    }

    /// Unified implementation for sending stress test messages via any protocol
    async fn send_stress_test_messages_impl(
        mut message_sender: MessageSender,
        args: &Args,
        peers: Vec<PeerId>,
        explore_config: &Option<ExploreConfiguration>,
    ) {
        let requested_size_bytes = args
            .message_size_bytes
            .expect("Even in explore mode message size should be set automatically.");
        let size_bytes =
            ensure_compatible_message_size(requested_size_bytes, &args.network_protocol);
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
                let message_clone = message.clone().into();
                let start_time = std::time::Instant::now();
                message_sender.send_message(&peers, message_clone).await;
                BROADCAST_MESSAGE_SEND_DELAY_SECONDS.record(start_time.elapsed().as_secs_f64());
                BROADCAST_MESSAGE_BYTES.set(message.len() as f64);
                BROADCAST_MESSAGE_COUNT.increment(1);
                BROADCAST_MESSAGE_BYTES_SUM.increment(message.len() as u64);
                trace!("Node {} sent message {message_index} in mode `{}`", args.id, args.mode);
                message_index += 1;
            }
        }
    }

    /// Starts the message receiving task
    pub async fn start_message_receiver(&mut self) -> BoxFuture<'static, ()> {
        let message_receiver = self.network_channels.take_receiver();

        async move {
            Self::receive_stress_test_messages_impl(message_receiver).await;
        }
        .boxed()
    }

    /// Implementation of the message receiving logic (moved from the standalone function)
    async fn receive_stress_test_messages_impl(message_receiver: MessageReceiver) {
        info!("Starting message receiver");
        message_receiver
            .for_each(|message| {
                tokio::task::spawn_blocking(|| receive_stress_test_message(message));
            })
            .await;
        info!("Message receiver task ended");
    }

    /// Starts the process metrics monitoring task
    pub fn start_metrics_monitor(&self) -> BoxFuture<'static, ()> {
        let metrics_interval = 1;
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

    async fn wait_for_next_running_phase(&mut self) {
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
    }

    /// Unified run function that handles both simple and network reset modes
    pub async fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        let test_timeout = Duration::from_secs(self.args.timeout);
        let start_time = tokio::time::Instant::now();

        self.wait_for_next_running_phase().await;

        // Main loop - restart if network reset is enabled, otherwise run once
        loop {
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
            if let Some(explore_config) = &mut self.explore_config {
                assert_eq!(explore_config.get_current_phase(), ExplorePhase::CoolDown);
            }

            self.wait_for_next_running_phase().await;
            // Recreate network manager for clean state
            self.recreate_network_manager().await;
        }
    }
}

pub async fn race_and_kill_tasks(spawned_tasks: Vec<JoinHandle<()>>) {
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

fn get_message(id: u64, size_bytes: usize) -> StressTestMessage {
    let message = StressTestMessage::new(id, 0, vec![0; size_bytes - *METADATA_SIZE]);
    assert_eq!(Vec::<u8>::from(message.clone()).len(), size_bytes);
    message
}

/// Ensures message size is compatible with the protocol
fn ensure_compatible_message_size(size_bytes: usize, protocol: &NetworkProtocol) -> usize {
    match protocol {
        NetworkProtocol::Propeller => {
            // Propeller requires messages to be multiples of 64 bytes
            let padded_size = size_bytes.div_ceil(64) * 64;
            if padded_size != size_bytes {
                info!(
                    "Propeller: Padding message size from {} to {} bytes (multiple of 64)",
                    size_bytes, padded_size
                );
            }
            padded_size
        }
        _ => size_bytes, // Other protocols don't have special requirements
    }
}

fn should_broadcast_round_robin(args: &Args) -> bool {
    let now_seconds = seconds_since_epoch();
    let round_duration_seconds =
        args.round_duration_seconds.expect("round_duration_seconds required for rr mode");
    let current_round = (now_seconds / round_duration_seconds) % args.num_nodes;
    args.id == current_round
}

fn create_peer_private_key(peer_index: u64) -> [u8; 32] {
    let array = peer_index.to_le_bytes();
    assert_eq!(array.len(), 8);
    let mut private_key = [0u8; 32];
    private_key[0..8].copy_from_slice(&array);
    private_key
}
