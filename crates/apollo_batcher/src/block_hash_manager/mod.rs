#![allow(dead_code, unused_variables)]

use starknet_api::block::BlockNumber;
use starknet_api::core::StateDiffCommitment;
use starknet_api::state::ThinStateDiff;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::task::JoinHandle;
use tracing::info;

use crate::block_hash_manager::state_committer::StateCommitter;
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
    pub(crate) fn initialize(tasks_channel_size: usize, results_channel_size: usize) -> Self {
        info!(
            "Initializing BlockHashManager with input channel size {} and results channel size {}",
            tasks_channel_size, results_channel_size
        );
        let (tasks_sender, tasks_receiver) = channel(tasks_channel_size);
        let (results_sender, results_receiver) = channel(results_channel_size);

        let state_committer = StateCommitter { tasks_receiver, results_sender };

        let commitment_task_performer = state_committer.run();

        Self { tasks_sender, results_receiver, commitment_task_performer }
    }

    pub(crate) async fn add_commitment_task(
        self,
        height: BlockNumber,
        state_diff: ThinStateDiff,
        state_diff_commitment: Option<StateDiffCommitment>,
    ) {
        self.tasks_sender
            .send(CommitmentTaskInput { height, state_diff, state_diff_commitment })
            .await
            .unwrap_or_else(|err| {
                panic!(
                    "Failed to send commitment task to StateCommitter. Block: {height} Error: \
                     {err}"
                );
            });
        info!("Sent commitment task for block {height} to StateCommitter.");
    }

    pub(crate) async fn get_commitment_results(&mut self) -> Vec<CommitmentTaskOutput> {
        unimplemented!()
    }

    // TODO(Amos): Pass committer client as argument.
    pub(crate) async fn revert_block(height: BlockNumber, reversed_state_diff: ThinStateDiff) {
        unimplemented!()
    }
}
