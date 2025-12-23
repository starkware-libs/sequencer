#![allow(dead_code, unused_variables)]

use apollo_batcher_config::config::BatcherConfig;
use apollo_reverts::RevertConfig;
use starknet_api::block::BlockNumber;
use starknet_api::block_hash::block_hash_calculator::PartialBlockHashComponents;
use starknet_api::core::StateDiffCommitment;
use starknet_api::state::ThinStateDiff;
use tokio::sync::mpsc::error::{TryRecvError, TrySendError};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::task::JoinHandle;
use tracing::info;

use crate::batcher::BatcherStorageReader;
use crate::commitment_manager::errors::CommitmentManagerError;
use crate::commitment_manager::state_committer::StateCommitter;
use crate::commitment_manager::types::{CommitmentTaskInput, CommitmentTaskOutput};

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

// TODO(amos): Sort methods and associated functions: public methods, private methods, public
// associated functions, private associated functions.
// TODO(amos): Think which methods / functions should be private.
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
        // TODO(Nimrod): Once this function is used, verify the calculated block hash of the first
        // new block with the value in the config.
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

    pub(crate) async fn read_commitment_input_and_add_task(
        height: BlockNumber,
        batcher_storage_reader: &dyn BatcherStorageReader,
        batcher_config: &BatcherConfig,
        commitment_manager: &mut Self,
    ) {
        let state_diff = match batcher_storage_reader.get_state_diff(height) {
            Ok(Some(diff)) => diff,
            Ok(None) => panic!("Missing state diff for height {height}."),
            Err(err) => panic!("Failed to read state diff for height {height}: {err}"),
        };
        let state_diff_commitment =
            if height < batcher_config.first_block_with_partial_block_hash.block_number {
                None
            } else {
                match batcher_storage_reader.get_partial_block_hash_components(height) {
                    Ok(Some(PartialBlockHashComponents { header_commitments, .. })) => {
                        Some(header_commitments.state_diff_commitment)
                    }
                    Ok(None) => panic!("Missing hash commitment for height {height}."),
                    Err(err) => panic!("Failed to read hash commitment for height {height}: {err}"),
                }
            };
        commitment_manager
            .add_commitment_task(height, state_diff, state_diff_commitment)
            .await
            .unwrap();
        info!(
            "Added commitment task for block {height}, {state_diff_commitment:?} to commitment \
             manager."
        );
    }

    /// Adds missing commitment tasks to the commitment manager. Missing tasks are caused by
    /// unfinished commitment tasks / results not written to storage when the sequencer is shut
    /// down.
    pub(crate) async fn add_missing_commitment_tasks(
        current_block_height: BlockNumber,
        batcher_config: &BatcherConfig,
        commitment_manager: &mut Self,
        batcher_storage_reader: &dyn BatcherStorageReader,
    ) {
        let start = commitment_manager.get_commitment_task_offset();
        let end = current_block_height;
        for height in start.iter_up_to(end) {
            Self::read_commitment_input_and_add_task(
                height,
                batcher_storage_reader,
                batcher_config,
                commitment_manager,
            )
            .await;
        }
        info!("Added missing commitment tasks for blocks [{start}, {end}) to commitment manager.");
    }

    /// If not in revert mode - creates and initializes the commitment manager, and also adds
    /// missing commitment tasks. Otherwise, returns None.
    pub(crate) async fn create_commitment_manager_or_none(
        batcher_config: &BatcherConfig,
        commitment_manager_config: &CommitmentManagerConfig,
        storage_reader: &dyn BatcherStorageReader,
    ) -> Option<Self> {
        let block_hash_height = storage_reader
            .block_hash_height()
            .expect("Failed to get block hash height from storage.");
        let mut commitment_manager = Self::new_or_none(
            commitment_manager_config,
            &batcher_config.revert_config,
            block_hash_height,
        );
        if let Some(ref mut cm) = commitment_manager {
            let block_height =
                storage_reader.height().expect("Failed to get block height from storage.");
            Self::add_missing_commitment_tasks(block_height, batcher_config, cm, storage_reader)
                .await;
        };
        commitment_manager
    }
}
