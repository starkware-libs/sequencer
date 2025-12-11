#![allow(dead_code)]

use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;

use crate::block_hash_manager::types::{CommitmentTaskInput, CommitmentTaskOutput};

pub(crate) mod state_committer;
pub(crate) mod types;

#[allow(dead_code)]
/// Encapsulates the block hash calculation logic.
pub(crate) struct BlockHashManager {
    pub(crate) tasks_sender: Sender<CommitmentTaskInput>,
    pub(crate) results_receiver: Receiver<CommitmentTaskOutput>,
    pub(crate) commitment_task_performer: JoinHandle<()>,
}

impl BlockHashManager {
    /// Initializes the BlockHashManager. This includes starting the state committer task.
    pub(crate) fn initialize(channel_size: usize) -> Self {
        let (tasks_sender, tasks_receiver) = tokio::sync::mpsc::channel(channel_size);
        let (results_sender, results_receiver) = tokio::sync::mpsc::channel(channel_size);

        let state_committer = state_committer::StateCommitter { tasks_receiver, results_sender };

        let commitment_task_performer = state_committer.run();

        Self { tasks_sender, results_receiver, commitment_task_performer }
    }
}
