use std::str::FromStr;
use std::time::Duration;

use apollo_network::network_manager::NetworkManager;
use apollo_network::NetworkConfig;
use apollo_network_benchmark::node_args::NodeArgs;
use futures::future::{select_all, BoxFuture};
use futures::FutureExt;
use libp2p::Multiaddr;
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::protocol::{register_protocol_channels, MessageReceiver, MessageSender};

/// The main stress test node that manages network communication and monitoring
pub struct BroadcastNetworkStressTestNode {
    args: NodeArgs,
    network_manager: Option<NetworkManager>,
    // TODO(AndrewL): Remove this once they are used
    #[allow(dead_code)]
    message_sender: Option<MessageSender>,
    // TODO(AndrewL): Remove this once they are used
    #[allow(dead_code)]
    message_receiver: Option<MessageReceiver>,
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

    /// Creates a new BroadcastNetworkStressTestNode instance
    pub async fn new(args: NodeArgs) -> Self {
        // Create network configuration
        let network_config = Self::create_network_config(&args);

        // Create network manager
        let mut network_manager = NetworkManager::new(network_config, None, None);

        // Register protocol channels
        let (message_sender, message_receiver) = register_protocol_channels(
            &mut network_manager,
            args.user.buffer_size,
            &args.user.network_protocol,
        );
        Self {
            args,
            network_manager: Some(network_manager),
            message_sender: Some(message_sender),
            message_receiver: Some(message_receiver),
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

    /// Gets all the tasks that need to be run
    async fn get_tasks(&mut self) -> Vec<BoxFuture<'static, ()>> {
        let mut tasks = Vec::new();
        tasks.push(self.start_network_manager().await);

        tasks
    }

    /// Unified run function that handles both simple and network reset modes
    pub async fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        let test_timeout = Duration::from_secs(self.args.user.timeout);
        let start_time = tokio::time::Instant::now();
        // Main loop - restart if network reset is enabled, otherwise run once

        info!("Starting/restarting all tasks");

        // Start all common tasks
        let tasks = self.get_tasks().await;

        // Wait for either timeout or any task completion
        let remaining_time = test_timeout.saturating_sub(start_time.elapsed());
        let spawned_tasks: Vec<_> = tasks.into_iter().map(|task| tokio::spawn(task)).collect();
        let task_completed =
            tokio::time::timeout(remaining_time, race_and_kill_tasks(spawned_tasks)).await.is_ok();

        if !task_completed {
            info!("Test timeout reached");
            return Err("Test timeout".into());
        }

        Err("Tasks should never end".into())
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
