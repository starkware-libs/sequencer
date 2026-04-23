use std::str::FromStr;
use std::time::Duration;

use apollo_network::network_manager::NetworkManager;
use apollo_network::NetworkConfig;
use apollo_network_benchmark::node_args::{Mode, NodeArgs};
use futures::future::{select_all, BoxFuture};
use futures::FutureExt;
use libp2p::Multiaddr;
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::handlers::{
    receive_stress_test_message,
    record_indexed_message,
    send_stress_test_messages,
};
use crate::metrics::create_network_metrics;
use crate::protocol::{register_protocol_channels, MessageReceiver, MessageSender};

/// The main stress test node that manages network communication and monitoring
pub struct StressTestNode {
    args: NodeArgs,
    network_manager: Option<NetworkManager>,
    message_sender: Option<MessageSender>,
    message_receiver: Option<MessageReceiver>,
}

impl StressTestNode {
    /// Creates network configuration from arguments
    fn create_network_config(args: &NodeArgs) -> NetworkConfig {
        let peer_private_key =
            apollo_network_benchmark::peer_key::private_key_from_node_id(args.runner.id);

        let mut network_config = NetworkConfig {
            port: args.runner.p2p_port,
            secret_key: Some(peer_private_key.to_vec().into()),
            ..Default::default()
        };

        // disable Kademlia discovery
        network_config.discovery_config.heartbeat_interval = Duration::from_secs(u64::MAX);

        if !args.runner.bootstrap.is_empty() {
            let bootstrap_peers: Vec<Multiaddr> = args
                .runner
                .bootstrap
                .iter()
                .map(|bootstrap_addr| {
                    Multiaddr::from_str(bootstrap_addr.trim())
                        .expect("invalid multiaddr in --bootstrap; check CLI/env input")
                })
                .collect();
            network_config.bootstrap_peer_multiaddr = Some(bootstrap_peers);
        }

        network_config
    }

    /// Creates a new StressTestNode instance
    pub async fn new(args: NodeArgs) -> Self {
        let network_config = Self::create_network_config(&args);

        let network_metrics = create_network_metrics();
        let mut network_manager = NetworkManager::new(network_config, None, Some(network_metrics));

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
        let network_manager = self
            .network_manager
            .take()
            .expect("network_manager is set in new() and taken at most once");
        async move {
            // Ignore network_manager result: race_and_kill_tasks aborts siblings when this
            // task ends, so surfacing the error here would be redundant noise.
            let _run_result = network_manager.run().await;
        }
        .boxed()
    }

    fn get_broadcaster_id(args: &NodeArgs) -> u64 {
        args.user
            .broadcaster
            .expect("clap's required_if_eq enforces broadcaster is Some when mode=one")
    }

    /// Determines if this node should broadcast messages based on the mode
    pub fn should_broadcast(&self) -> bool {
        match self.args.user.mode {
            Mode::AllBroadcast | Mode::RoundRobin => true,
            Mode::OneBroadcast => {
                let broadcaster_id = Self::get_broadcaster_id(&self.args);
                self.args.runner.id == broadcaster_id
            }
        }
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

        let message_sender = self
            .message_sender
            .take()
            .expect("message_sender is set in new() and taken at most once");

        let args = self.args.clone();

        Some(
            async move {
                send_stress_test_messages(message_sender, &args).await;
            }
            .boxed(),
        )
    }

    /// Starts the message receiving tasks (receiver + index tracker)
    pub async fn make_message_receiver_tasks(&mut self) -> Vec<BoxFuture<'static, ()>> {
        let message_receiver = self
            .message_receiver
            .take()
            .expect("message_receiver is set in new() and taken at most once");

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
                    .for_each(|message, peer_id| {
                        let tx_clone = tx_clone.clone();
                        receive_stress_test_message(message, peer_id, tx_clone);
                    })
                    .await;
                info!("Message receiver task ended");
            }
            .boxed(),
        ]
    }

    /// Gets all the tasks that need to be run
    async fn get_tasks(&mut self) -> Vec<BoxFuture<'static, ()>> {
        let mut tasks = Vec::new();
        tasks.push(self.start_network_manager().await);
        tasks.extend(self.make_message_receiver_tasks().await);

        if let Some(sender_task) = self.start_message_sender().await {
            tasks.push(sender_task);
        }

        tasks
    }

    pub async fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        let test_timeout = Duration::from_secs(self.args.user.timeout);
        let start_time = tokio::time::Instant::now();

        info!("Starting all tasks");

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
