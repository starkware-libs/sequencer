use std::str::FromStr;
use std::time::Duration;

use apollo_network::network_manager::NetworkManager;
use apollo_network::NetworkConfig;
use apollo_network_benchmark::node_args::{Mode, NodeArgs};
use futures::future::{select_all, BoxFuture};
use futures::FutureExt;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::{Multiaddr, PeerId};
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::explore_config::{extract_explore_params, ExploreConfiguration, ExplorePhase};
use crate::handlers::{
    receive_stress_test_message,
    record_indexed_message,
    send_stress_test_messages_impl,
};
use crate::metrics::{create_network_metrics, BROADCAST_MESSAGE_THROUGHPUT, NETWORK_RESET_TOTAL};
use crate::protocol::{register_protocol_channels, MessageReceiver, MessageSender};

/// The main stress test node that manages network communication and monitoring
pub struct BroadcastNetworkStressTestNode {
    args: NodeArgs,
    network_config: NetworkConfig,
    network_manager: Option<NetworkManager>,
    message_sender: Option<MessageSender>,
    message_receiver: Option<MessageReceiver>,
    explore_config: Option<ExploreConfiguration>,
}

impl BroadcastNetworkStressTestNode {
    /// Creates network configuration from arguments
    fn create_network_config(args: &NodeArgs) -> NetworkConfig {
        let peer_private_key = create_peer_private_key(args.runner.id);
        let peer_private_key_hex =
            peer_private_key.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
        info!("Secret Key: {peer_private_key_hex:#?}");

        let mut network_config = NetworkConfig {
            port: args.runner.p2p_port,
            secret_key: Some(peer_private_key.to_vec().into()),
            ..Default::default()
        };

        network_config.discovery_config.heartbeat_interval = Duration::from_secs(99999999);

        if !args.runner.bootstrap.is_empty() {
            let bootstrap_peers: Vec<Multiaddr> = args
                .runner
                .bootstrap
                .iter()
                .map(|s| Multiaddr::from_str(s.trim()).unwrap())
                .collect();
            network_config.bootstrap_peer_multiaddr = Some(bootstrap_peers);
        }

        network_config
    }

    /// Creates explore configuration and initializes message parameters
    fn setup_explore_config(args: &NodeArgs) -> Option<ExploreConfiguration> {
        if let Mode::Explore = args.user.mode {
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
    pub async fn new(args: NodeArgs) -> Self {
        // Create network configuration
        let network_config = Self::create_network_config(&args);

        // Create network manager
        let network_metrics = create_network_metrics();
        let mut network_manager =
            NetworkManager::new(network_config.clone(), None, Some(network_metrics));

        // Register protocol channels
        let (message_sender, message_receiver) = register_protocol_channels(
            &mut network_manager,
            args.user.buffer_size,
            &args.user.network_protocol,
            &args.runner.bootstrap,
        )
        .await;

        // Setup explore configuration if needed
        let explore_config = Self::setup_explore_config(&args);

        Self {
            args,
            network_config,
            network_manager: Some(network_manager),
            message_sender: Some(message_sender),
            message_receiver: Some(message_receiver),
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
        // Create new network manager
        let network_metrics = create_network_metrics();
        let mut network_manager =
            NetworkManager::new(self.network_config.clone(), None, Some(network_metrics));

        // Register protocol channels
        let (message_sender, message_receiver) = register_protocol_channels(
            &mut network_manager,
            self.args.user.buffer_size,
            &self.args.user.network_protocol,
            &self.args.runner.bootstrap,
        )
        .await;

        info!("Recreated Network Manager");

        // Update the struct with new components
        self.network_manager = Some(network_manager);
        self.message_sender = Some(message_sender);
        self.message_receiver = Some(message_receiver);
    }

    /// Gets the broadcaster ID with validation for modes that require it
    fn get_broadcaster_id(args: &NodeArgs) -> u64 {
        args.user.broadcaster.expect("broadcaster required for one/explore mode")
    }

    /// Determines if this node should broadcast messages based on the mode
    pub fn should_broadcast(&self) -> bool {
        match self.args.user.mode {
            Mode::AllBroadcast | Mode::RoundRobin => true,
            Mode::OneBroadcast | Mode::Explore => {
                let broadcaster_id = Self::get_broadcaster_id(&self.args);
                self.args.runner.id == broadcaster_id
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
            info!(
                "Node {} will NOT broadcast in mode `{}`",
                self.args.runner.id, self.args.user.mode
            );
            return None;
        }

        info!("Node {} will broadcast in mode `{}`", self.args.runner.id, self.args.user.mode);

        let message_sender =
            self.message_sender.take().expect("message_sender should be available");
        let args_clone = self.args.clone();
        let explore_config = self.explore_config.clone();
        let peers = self.get_peers();

        Some(
            async move {
                send_stress_test_messages_impl(message_sender, &args_clone, peers, &explore_config)
                    .await;
            }
            .boxed(),
        )
    }

    /// Starts the message receiving task
    pub fn start_message_receiver(&mut self) -> Vec<BoxFuture<'static, ()>> {
        let message_receiver =
            self.message_receiver.take().expect("message_receiver should be available");

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let num_peers = self.args.runner.bootstrap.len();

        vec![
            async move {
                record_indexed_message(rx, num_peers).await;
            }
            .boxed(),
            async move {
                info!("Starting message receiver");
                let tx_clone = tx.clone();
                message_receiver
                    .for_each(|message, _| {
                        let tx_clone = tx_clone.clone();
                        receive_stress_test_message(message, tx_clone);
                    })
                    .await;
                info!("Message receiver task ended");
            }
            .boxed(),
        ]
    }

    /// Sets up and starts all tasks common to both simple and network reset modes
    async fn setup_tasks(&mut self) -> Vec<BoxFuture<'static, ()>> {
        let mut tasks = Vec::new();
        tasks.push(self.start_network_manager().await);
        tasks.extend(self.start_message_receiver());

        if let Some(sender_task) = self.start_message_sender().await {
            tasks.push(sender_task);
        }

        tasks
    }

    async fn wait_for_next_running_phase(&mut self) {
        if let Some(explore_config) = &mut self.explore_config {
            if self.args.runner.id == Self::get_broadcaster_id(&self.args) {
                BROADCAST_MESSAGE_THROUGHPUT.set(0);
            }
            while explore_config.get_current_phase() == ExplorePhase::CoolDown {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            let (size, duration) = explore_config.get_current_size_and_heartbeat();
            self.args.user.message_size_bytes = size;
            self.args.user.heartbeat_millis = duration.as_millis().try_into().unwrap();
        }
    }

    /// Unified run function that handles both simple and network reset modes
    pub async fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        let test_timeout = Duration::from_secs(self.args.user.timeout);
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

fn create_peer_private_key(peer_index: u64) -> [u8; 32] {
    let array = peer_index.to_le_bytes();
    assert_eq!(array.len(), 8);
    let mut private_key = [0u8; 32];
    private_key[0..8].copy_from_slice(&array);
    private_key
}
