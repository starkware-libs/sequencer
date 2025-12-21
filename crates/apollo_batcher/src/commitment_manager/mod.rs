#![allow(dead_code, unused_variables)]

use apollo_reverts::RevertConfig;
use starknet_api::block::BlockNumber;
use starknet_api::core::StateDiffCommitment;
use starknet_api::state::ThinStateDiff;
use tokio::sync::mpsc::error::{TryRecvError, TrySendError};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::task::JoinHandle;
use tracing::info;

use crate::commitment_manager::state_committer::StateCommitter;
use crate::commitment_manager::types::{CommitmentTaskInput, CommitmentTaskOutput};

pub(crate) mod state_committer;
pub(crate) mod types;
pub(crate) mod utils;

pub(crate) const DEFAULT_TASKS_CHANNEL_SIZE: usize = 1000;
pub(crate) const DEFAULT_RESULTS_CHANNEL_SIZE: usize = 1000;

// TODO(amos): Add to Batcher config.
#[derive(Debug)]
pub(crate) struct CommitmentManagerConfig {
    pub(crate) tasks_channel_size: usize,
    pub(crate) results_channel_size: usize,
    // Wait for tasks channel to be available before sending.
    pub(crate) wait_for_tasks_channel: bool,
}

impl Default for CommitmentManagerConfig {
    fn default() -> Self {
        Self {
            tasks_channel_size: DEFAULT_TASKS_CHANNEL_SIZE,
            results_channel_size: DEFAULT_RESULTS_CHANNEL_SIZE,
            wait_for_tasks_channel: true,
        }
    }
}

#[allow(dead_code)]
/// Encapsulates the block hash calculation logic.
pub(crate) struct CommitmentManager {
    pub(crate) tasks_sender: Sender<CommitmentTaskInput>,
    pub(crate) results_receiver: Receiver<CommitmentTaskOutput>,
    pub(crate) commitment_task_performer: JoinHandle<()>,
    pub(crate) config: CommitmentManagerConfig,
    pub(crate) commitment_task_offset: BlockNumber,
}

impl CommitmentManager {
    /// Initializes and returns the Commitment manager, or None when in revert mode.
    pub(crate) fn new_or_none(
        config: &CommitmentManagerConfig,
        revert_config: &RevertConfig,
        block_hash_height: BlockNumber,
    ) -> Option<Self> {
        if revert_config.should_revert {
            info!("Revert mode is enabled, not initializing commitment manager.");
            None
        } else {
            info!("Initializing commitment manager.");
            Some(CommitmentManager::initialize(
                CommitmentManagerConfig::default(),
                block_hash_height,
            ))
        }
    }

    /// Initializes the CommitmentManager. This includes starting the state committer task.
    pub(crate) fn initialize(
        config: CommitmentManagerConfig,
        block_hash_height: BlockNumber,
    ) -> Self {
        info!("Initializing CommitmentManager with config {config:?}");
        let (tasks_sender, tasks_receiver) = channel(config.tasks_channel_size);
        let (results_sender, results_receiver) = channel(config.results_channel_size);

        let state_committer = StateCommitter { tasks_receiver, results_sender };

        let commitment_task_performer = state_committer.run();

        Self {
            tasks_sender,
            results_receiver,
            commitment_task_performer,
            config,
            commitment_task_offset: block_hash_height,
        }
    }

    /// Returns the range of block heights for which commitment tasks are missing (i.e.,
    /// [commitment_task_offset, current_block_height)).
    /// The returned range is inclusive of the start and exclusive of the end.
    pub(crate) fn get_missing_commitment_tasks_heights(
        &self,
        current_block_height: BlockNumber,
    ) -> (BlockNumber, BlockNumber) {
        (self.commitment_task_offset, current_block_height)
    }

    pub(crate) async fn add_commitment_task(
        &mut self,
        height: BlockNumber,
        state_diff: ThinStateDiff,
        state_diff_commitment: Option<StateDiffCommitment>,
    ) {
        assert!(
            height == self.commitment_task_offset,
            "Attempted to add commitment task for block {height}, but the expected block height \
             is {}. Commitment tasks must be added in order.",
            self.commitment_task_offset
        );
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
                Ok(_) => {
                    info!(success_message);
                    self.increase_commitment_task_offset();
                }
                Err(err) => panic!("{error_message}. error: {err}"),
            };
        } else {
            match self.tasks_sender.try_send(commitment_task_input) {
                Ok(_) => {
                    info!(success_message);
                    self.increase_commitment_task_offset();
                }
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

    /// Fetches all ready commitment results from the state committer.
    pub(crate) async fn get_commitment_results(&mut self) -> Vec<CommitmentTaskOutput> {
        let mut results = Vec::new();
        loop {
            match self.results_receiver.try_recv() {
                Ok(result) => results.push(result),
                Err(TryRecvError::Empty) => break,
                Err(err) => {
                    panic!("Failed to receive commitment result from state committer. error: {err}")
                }
            }
        }
        results
    }

    pub(crate) async fn revert_block(height: BlockNumber, reversed_state_diff: ThinStateDiff) {
        unimplemented!()
    }

    fn increase_commitment_task_offset(&mut self) {
        self.commitment_task_offset =
            self.commitment_task_offset.next().expect("Block number overflowed.");
    }
}
