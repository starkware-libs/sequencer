use std::time::Duration;

use apollo_network_benchmark::node_args::NodeArgs;
use futures::future::{select_all, BoxFuture};
use tokio::task::JoinHandle;
use tracing::{info, warn};

/// The main stress test node that manages network communication and monitoring
pub struct BroadcastNetworkStressTestNode {
    args: NodeArgs,
}

impl BroadcastNetworkStressTestNode {
    /// Creates a new BroadcastNetworkStressTestNode instance
    pub async fn new(args: NodeArgs) -> Self {
        Self { args }
    }

    /// Gets all the tasks that need to be run
    async fn get_tasks(&mut self) -> Vec<BoxFuture<'static, ()>> {
        Vec::new()
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
