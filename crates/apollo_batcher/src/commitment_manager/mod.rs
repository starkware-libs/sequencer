#![allow(dead_code, unused_variables)]

use std::sync::Arc;

use apollo_reverts::RevertConfig;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::block_hash::block_hash_calculator::calculate_block_hash;
use starknet_api::core::StateDiffCommitment;
use starknet_api::state::ThinStateDiff;
use tokio::sync::mpsc::error::{TryRecvError, TrySendError};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::task::JoinHandle;
use tracing::info;

use crate::batcher::BatcherStorageReader;
use crate::commitment_manager::errors::CommitmentManagerError;
use crate::commitment_manager::state_committer::StateCommitter;
use crate::commitment_manager::types::{
    CommitmentTaskInput,
    CommitmentTaskOutput,
    FinalBlockCommitment,
};

pub(crate) mod errors;
pub(crate) mod state_committer;
pub(crate) mod types;

pub(crate) const DEFAULT_TASKS_CHANNEL_SIZE: usize = 1000;
pub(crate) const DEFAULT_RESULTS_CHANNEL_SIZE: usize = 1000;

pub(crate) type CommitmentManagerResult<T> = Result<T, CommitmentManagerError>;

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
            None
        } else {
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

    pub(crate) fn get_commitment_task_offset(&self) -> BlockNumber {
        self.commitment_task_offset
    }

    /// Adds a commitment task to the state committer. If the task height does not match the
    /// task offset, an error is returned. If the tasks channel is full, the behavior depends on
    /// the config: if `wait_for_tasks_channel` is true, it will wait until there is space in the
    /// channel; otherwise, it will panic. Any other error when sending the task will also cause a
    /// panic.
    pub(crate) async fn add_commitment_task(
        &mut self,
        height: BlockNumber,
        state_diff: ThinStateDiff,
        state_diff_commitment: Option<StateDiffCommitment>,
    ) -> CommitmentManagerResult<()> {
        if height != self.commitment_task_offset {
            return Err(CommitmentManagerError::WrongTaskHeight {
                expected: self.commitment_task_offset,
                actual: height,
                state_diff_commitment,
            });
        }
        let commitment_task_input =
            CommitmentTaskInput { height, state_diff, state_diff_commitment };
        let error_message = format!(
            "Failed to send commitment task to state committer. Block: {height}, state diff \
             commitment: {state_diff_commitment:?}",
        );

        if self.config.wait_for_tasks_channel {
            info!(
                "Waiting to send commitment task for block {height} and state diff \
                 {state_diff_commitment:?} to state committer."
            );
            match self.tasks_sender.send(commitment_task_input).await {
                Ok(_) => self.successfully_added_commitment_task(height, state_diff_commitment),
                Err(err) => panic!("{error_message}. error: {err}"),
            }
        } else {
            match self.tasks_sender.try_send(commitment_task_input) {
                Ok(_) => self.successfully_added_commitment_task(height, state_diff_commitment),
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
            }
        }
    }

    fn successfully_added_commitment_task(
        &mut self,
        height: BlockNumber,
        state_diff_commitment: Option<StateDiffCommitment>,
    ) -> CommitmentManagerResult<()> {
        info!(
            "Sent commitment task for block {height} and state diff {state_diff_commitment:?} to \
             state committer."
        );
        self.increase_commitment_task_offset();
        Ok(())
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

    /// Returns the final commitment output for a given commitment task output.
    /// If `should_finalize_block_hash` is true, finalizes the commitment by calculating the block
    /// hash using the global root, the parent block hash and the partial block hash components.
    /// Otherwise, returns the final commitment with no block hash.
    pub(crate) fn final_commitment_output<R: BatcherStorageReader + ?Sized>(
        storage_reader: Arc<R>,
        CommitmentTaskOutput { height, global_root }: CommitmentTaskOutput,
        should_finalize_block_hash: bool,
    ) -> CommitmentManagerResult<FinalBlockCommitment> {
        match should_finalize_block_hash {
            false => {
                info!("Finalized commitment for block {height} without calculating block hash.");
                Ok(FinalBlockCommitment { height, block_hash: None, global_root })
            }
            true => {
                info!("Finalizing commitment for block {height} with calculating block hash.");

                // TODO(Nimrod): Extend the storage reader to fetch both parent hash and partial
                // components in a single tx and use it here.
                let parent_hash = match height.prev() {
                    None => {
                        // The parent hash of the genesis block is zero.
                        BlockHash::default()
                    }
                    Some(parent_height) => storage_reader
                        .get_block_hash(parent_height)?
                        .ok_or(CommitmentManagerError::MissingBlockHash(parent_height))?,
                };
                let partial_block_hash_components = storage_reader
                    .get_partial_block_hash_components(height)?
                    .ok_or(CommitmentManagerError::MissingPartialBlockHashComponents(height))?;
                let block_hash =
                    calculate_block_hash(&partial_block_hash_components, global_root, parent_hash)?;
                Ok(FinalBlockCommitment { height, block_hash: Some(block_hash), global_root })
            }
        }
    }
}
