#![allow(dead_code)]

use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::task::JoinHandle;
use tracing::info;

use crate::block_hash_manager::state_committer::StateCommitter;
use crate::block_hash_manager::types::{CommitmentTaskInput, CommitmentTaskOutput};

pub(crate) mod state_committer;
pub(crate) mod types;

// TODO(amos): Add to Batcher config.
#[derive(Debug)]
pub(crate) struct BlockHashManagerConfig {
    pub(crate) tasks_channel_size: usize,
    pub(crate) results_channel_size: usize,
}

#[allow(dead_code)]
/// Encapsulates the block hash calculation logic.
pub(crate) struct BlockHashManager {
    pub(crate) tasks_sender: Sender<CommitmentTaskInput>,
    pub(crate) results_receiver: Receiver<CommitmentTaskOutput>,
    pub(crate) commitment_task_performer: JoinHandle<()>,
    pub(crate) config: BlockHashManagerConfig,
}

impl BlockHashManager {
    /// Initializes the BlockHashManager. This includes starting the state committer task.
    pub(crate) fn initialize(config: BlockHashManagerConfig) -> Self {
        info!("Initializing BlockHashManager with config {config:?}");
        let (tasks_sender, tasks_receiver) = channel(config.tasks_channel_size);
        let (results_sender, results_receiver) = channel(config.results_channel_size);

        let state_committer = StateCommitter { tasks_receiver, results_sender };

        let commitment_task_performer = state_committer.run();

        Self { tasks_sender, results_receiver, commitment_task_performer, config }
    }
}
