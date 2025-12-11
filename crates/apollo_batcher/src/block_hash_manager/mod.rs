#![allow(dead_code, unused_variables)]

use core::panic;

use starknet_api::block::BlockNumber;
use starknet_api::core::StateDiffCommitment;
use starknet_api::state::ThinStateDiff;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::task::JoinHandle;
use tracing::info;

use crate::block_hash_manager::state_committer::StateCommitter;
use crate::block_hash_manager::types::{CommitmentTaskInput, CommitmentTaskOutput};

pub(crate) mod state_committer;
pub(crate) mod types;

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
}

impl BlockHashManager {
    /// Initializes the BlockHashManager. This includes starting the state committer task.
    pub(crate) fn initialize(config: BlockHashManagerConfig) -> Self {
        info!(
            "Initializing BlockHashManager with input channel size {} and results channel size {}",
            config.tasks_channel_size, config.results_channel_size
        );
        let (tasks_sender, tasks_receiver) = channel(config.tasks_channel_size);
        let (results_sender, results_receiver) = channel(config.results_channel_size);

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
        match self.tasks_sender.try_send(CommitmentTaskInput {
            height,
            state_diff,
            state_diff_commitment,
        }) {
            Ok(_) => {
                info!(
                    "Sent commitment task for block {height} and state diff \
                     {state_diff_commitment:?} to StateCommitter."
                );
            }
            Err(TrySendError::Full(_)) => {
                let channel_size = self.tasks_sender.max_capacity();
                panic!(
                    "Failed to send commitment task to StateCommitter because the channel is \
                     full. Block: {height}, state_diff_commitment: {state_diff_commitment:?}, \
                     channel size: {channel_size}",
                );
            }
            Err(err) => {
                panic!(
                    "Failed to send commitment task to StateCommitter. error: {err}, block: \
                     {height}, state_diff_commitment: {state_diff_commitment:?}",
                );
            }
        };
    }

    pub(crate) async fn get_commitment_result(&mut self) -> CommitmentTaskOutput {
        unimplemented!()
    }

    // TODO(Amos): Pass committer client as argument.
    pub(crate) async fn revert_block(height: BlockNumber, reversed_state_diff: ThinStateDiff) {
        unimplemented!()
    }
}
