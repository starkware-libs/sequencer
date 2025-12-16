#![allow(dead_code, unused_variables)]

use starknet_api::block::BlockNumber;
use starknet_api::core::StateDiffCommitment;
use starknet_api::state::ThinStateDiff;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::task::JoinHandle;
use tracing::info;

use crate::commitment_manager::state_committer::StateCommitter;
use crate::commitment_manager::types::{CommitmentTaskInput, CommitmentTaskOutput};

pub(crate) mod state_committer;
pub(crate) mod types;

// TODO(amos): Add to Batcher config.
#[derive(Debug)]
pub(crate) struct CommitmentManagerConfig {
    pub(crate) tasks_channel_size: usize,
    pub(crate) results_channel_size: usize,
    // Wait for tasks channel to be available before sending.
    pub(crate) wait_for_tasks_channel: bool,
}

#[allow(dead_code)]
/// Encapsulates the block hash calculation logic.
pub(crate) struct CommitmentManager {
    pub(crate) tasks_sender: Sender<CommitmentTaskInput>,
    pub(crate) results_receiver: Receiver<CommitmentTaskOutput>,
    pub(crate) commitment_task_performer: JoinHandle<()>,
    pub(crate) config: CommitmentManagerConfig,
}

impl CommitmentManager {
    /// Initializes the CommitmentManager. This includes starting the state committer task.
    pub(crate) fn initialize(config: CommitmentManagerConfig) -> Self {
        info!("Initializing CommitmentManager with config {config:?}");
        let (tasks_sender, tasks_receiver) = channel(config.tasks_channel_size);
        let (results_sender, results_receiver) = channel(config.results_channel_size);

        let state_committer = StateCommitter { tasks_receiver, results_sender };

        let commitment_task_performer = state_committer.run();

        Self { tasks_sender, results_receiver, commitment_task_performer, config }
    }

    pub(crate) async fn add_commitment_task(
        &self,
        height: BlockNumber,
        state_diff: ThinStateDiff,
        state_diff_commitment: Option<StateDiffCommitment>,
    ) {
        let commitment_task_input =
            CommitmentTaskInput { height, state_diff, state_diff_commitment };
        let error_message = format!(
            "Failed to send commitment task to state committer. Block: {height}, state diff \
             commitment: {state_diff_commitment:?}",
        );
        let success_message = format!(
            "Sent commitment task for block {height} and state diff {state_diff_commitment:?} to \
             state committer.",
        );

        if self.config.wait_for_tasks_channel {
            info!(
                "Waiting to send commitment task for block {height} and state diff \
                 {state_diff_commitment:?} to state committer."
            );
            match self.tasks_sender.send(commitment_task_input).await {
                Ok(_) => info!(success_message),
                Err(err) => panic!("{error_message}. error: {err}"),
            };
        } else {
            match self.tasks_sender.try_send(commitment_task_input) {
                Ok(_) => info!(success_message),
                Err(TrySendError::Full(_)) => {
                    let channel_size = self.tasks_sender.max_capacity();
                    panic!(
                        "Failed to send commitment task to state committer because the channel is \
                         full. Block: {height}, state diff commitment: {state_diff_commitment:?}, \
                         channel size: {channel_size}. Consider increasing the channel size or \
                         enabling waiting in the config.",
                    );
                }
                Err(err) => panic!("{error_message}. error: {err}"),
            };
        }
    }

    pub(crate) async fn get_commitment_results(&mut self) -> Vec<CommitmentTaskOutput> {
        unimplemented!()
    }

    // TODO(Amos): Pass committer client as argument.
    pub(crate) async fn revert_block(height: BlockNumber, reversed_state_diff: ThinStateDiff) {
        unimplemented!()
    }
}
