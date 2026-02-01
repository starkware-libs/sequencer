#![allow(dead_code, unused_variables)]

use std::sync::Arc;

use apollo_batcher_config::config::{
    BatcherConfig,
    CommitmentManagerConfig,
    FirstBlockWithPartialBlockHash,
};
use apollo_committer_types::committer_types::{
    CommitBlockRequest,
    CommitBlockResponse,
    RevertBlockRequest,
};
use apollo_committer_types::communication::{CommitterRequestLabelValue, SharedCommitterClient};
use starknet_api::block::BlockNumber;
use starknet_api::block_hash::block_hash_calculator::{
    calculate_block_hash,
    PartialBlockHashComponents,
};
use starknet_api::core::StateDiffCommitment;
use starknet_api::state::ThinStateDiff;
use tokio::sync::mpsc::error::{TryRecvError, TrySendError};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::time::{sleep, Duration};
use tracing::info;

use crate::batcher::{BatcherStorageReader, BatcherStorageWriter};
use crate::commitment_manager::errors::CommitmentManagerError;
use crate::commitment_manager::state_committer::{StateCommitter, StateCommitterTrait};
use crate::commitment_manager::types::{
    CommitmentTaskOutput,
    CommitterTaskInput,
    CommitterTaskOutput,
    FinalBlockCommitment,
    RevertTaskOutput,
    TaskTimer,
};
use crate::metrics::{
    COMMITMENT_MANAGER_COMMIT_BLOCK_LATENCY,
    COMMITMENT_MANAGER_COMMIT_BLOCK_LATENCY_HIST,
    COMMITMENT_MANAGER_NUM_COMMIT_RESULTS_HIST,
};

pub(crate) type CommitmentManagerResult<T> = Result<T, CommitmentManagerError>;
pub(crate) type ApolloCommitmentManager = CommitmentManager<StateCommitter>;

#[allow(dead_code)]
/// Encapsulates the block hash calculation logic.
pub(crate) struct CommitmentManager<S: StateCommitterTrait> {
    pub(crate) tasks_sender: Sender<CommitterTaskInput>,
    pub(crate) results_receiver: Receiver<CommitterTaskOutput>,
    pub(crate) config: CommitmentManagerConfig,
    pub(crate) commitment_task_offset: BlockNumber,
    pub(crate) state_committer: S,
    pub(crate) task_timer: TaskTimer,
}

impl<S: StateCommitterTrait> CommitmentManager<S> {
    // Public methods.

    /// Creates and initializes the commitment manager, and also adds
    /// missing commitment tasks.
    pub(crate) async fn create_commitment_manager<
        R: BatcherStorageReader + ?Sized,
        W: BatcherStorageWriter + ?Sized,
    >(
        batcher_config: &BatcherConfig,
        commitment_manager_config: &CommitmentManagerConfig,
        storage_reader: Arc<R>,
        storage_writer: &mut Box<W>,
        committer_client: SharedCommitterClient,
    ) -> Self {
        let global_root_height = storage_reader
            .global_root_height()
            .expect("Failed to get global root height from storage.");
        info!("Initializing commitment manager.");
        let mut commitment_manager = CommitmentManager::initialize(
            commitment_manager_config,
            global_root_height,
            committer_client,
        );
        let block_height =
            storage_reader.state_diff_height().expect("Failed to get block height from storage.");
        commitment_manager
            .add_missing_commitment_tasks(
                block_height,
                batcher_config,
                storage_reader,
                storage_writer,
            )
            .await;
        commitment_manager
    }

    pub(crate) fn get_commitment_task_offset(&self) -> BlockNumber {
        self.commitment_task_offset
    }

    /// Adds a commitment task to the state committer. If the task height does not match the
    /// task offset, an error is returned.
    pub(crate) async fn add_commitment_task<
        R: BatcherStorageReader + ?Sized,
        W: BatcherStorageWriter + ?Sized,
    >(
        &mut self,
        height: BlockNumber,
        state_diff: ThinStateDiff,
        state_diff_commitment: Option<StateDiffCommitment>,
        first_block_with_partial_block_hash: &Option<FirstBlockWithPartialBlockHash>,
        storage_reader: Arc<R>,
        storage_writer: &mut Box<W>,
    ) -> CommitmentManagerResult<()> {
        if height != self.commitment_task_offset {
            return Err(CommitmentManagerError::WrongCommitmentTaskHeight {
                expected: self.commitment_task_offset,
                actual: height,
                state_diff_commitment,
            });
        }
        let commitment_task_input = CommitterTaskInput::Commit(CommitBlockRequest {
            height,
            state_diff,
            state_diff_commitment,
        });
        self.add_task(
            commitment_task_input,
            first_block_with_partial_block_hash,
            storage_reader,
            storage_writer,
        )
        .await?;
        self.successfully_added_commitment_task(height, state_diff_commitment);
        Ok(())
    }

    async fn add_task_if_channel_not_full(
        &mut self,
        task_input: CommitterTaskInput,
    ) -> Result<(), TrySendError<CommitterTaskInput>> {
        let task_height = task_input.height();
        let task_type = task_input.task_type();
        match self.tasks_sender.try_send(task_input) {
            Ok(_) => Ok(()),
            Err(TrySendError::Full(err)) => Err(TrySendError::Full(err))?,
            Err(err) => panic!(
                "Failed to send task to state committer: task height {task_height}, task type: \
                 {task_type:?}. \n error: {err}"
            ),
        }
    }

    /// If the tasks channel is full, the behavior depends on the config: if
    /// `panic_if_task_channel_full` is true, it will panic; otherwise, it will retry after reading
    /// results from the tasks channel. Any other error when sending the task will also cause a
    /// panic.
    async fn add_task<R: BatcherStorageReader + ?Sized, W: BatcherStorageWriter + ?Sized>(
        &mut self,
        task_input: CommitterTaskInput,
        first_block_with_partial_block_hash: &Option<FirstBlockWithPartialBlockHash>,
        storage_reader: Arc<R>,
        storage_writer: &mut Box<W>,
    ) -> CommitmentManagerResult<()> {
        loop {
            let result = self.add_task_if_channel_not_full(task_input.clone()).await;
            match result {
                Ok(_) => return Ok(()),
                Err(TrySendError::Full(err)) => {
                    let channel_size = self.tasks_sender.max_capacity();
                    let channel_is_full_msg = format!(
                        "The commitment manager tasks channel is full. channel size: \
                         {channel_size}.\n"
                    );
                    if self.config.panic_if_task_channel_full {
                        panic!(
                            "{channel_is_full_msg} Panicking because `panic_if_task_channel_full` \
                             is set to true.",
                        );
                    } else {
                        info!(
                            "{channel_is_full_msg} Will retry after reading results from the \
                             tasks channel."
                        );
                        sleep(Duration::from_millis(100)).await;
                        self.get_and_write_commitment_results_to_storage(
                            first_block_with_partial_block_hash,
                            storage_reader.clone(),
                            storage_writer,
                        )
                        .await?;
                    }
                }
                _ => unreachable!(),
            }
        } else {
            info!("Waiting to send task for {task_input} to state committer.");
            self.tasks_sender
                .send(task_input)
                .await
                .unwrap_or_else(|err| panic!("{error_message}. error: {err}"));
        }
    }

    /// Fetches all ready commitment results from the state committer. Panics if any task is a
    /// revert.
    pub(crate) async fn get_commitment_results(&mut self) -> Vec<CommitmentTaskOutput> {
        let mut results = Vec::new();
        loop {
            match self.results_receiver.try_recv() {
                Ok(result) => {
                    let task_duration = self
                        .task_timer
                        .stop_timer(CommitterRequestLabelValue::CommitBlock, result.height());
                    if let Some(task_duration) = task_duration {
                        // TODO(Rotem): add panels in the dashboard for the latency metrics.
                        COMMITMENT_MANAGER_COMMIT_BLOCK_LATENCY_HIST.record_lossy(task_duration);
                        COMMITMENT_MANAGER_COMMIT_BLOCK_LATENCY.set_lossy(task_duration);
                    }
                    results.push(result.expect_commitment())
                }
                Err(TryRecvError::Empty) => break,
                Err(err) => {
                    panic!("Failed to receive commitment result from state committer. error: {err}")
                }
            }
        }
        COMMITMENT_MANAGER_NUM_COMMIT_RESULTS_HIST.record_lossy(results.len());
        results
    }

    /// Fetches all ready commitment results from the state committer, until a revert result is
    /// received.
    pub(crate) async fn wait_for_revert_result(
        &mut self,
    ) -> (Vec<CommitmentTaskOutput>, RevertTaskOutput) {
        let mut commitment_results = Vec::new();
        loop {
            // Sleep until a message is sent or the channel is closed.
            match self.results_receiver.recv().await {
                Some(CommitterTaskOutput::Commit(commitment_task_result)) => {
                    commitment_results.push(commitment_task_result)
                }
                Some(CommitterTaskOutput::Revert(revert_task_result)) => {
                    return (commitment_results, revert_task_result);
                }
                None => panic!("Channel closed while waiting for revert results."),
            }
        }
    }

    pub(crate) async fn write_commitment_results_to_storage<
        R: BatcherStorageReader + ?Sized,
        W: BatcherStorageWriter + ?Sized,
    >(
        &mut self,
        commitment_results: Vec<CommitmentTaskOutput>,
        first_block_with_partial_block_hash: &Option<FirstBlockWithPartialBlockHash>,
        storage_reader: Arc<R>,
        storage_writer: &mut Box<W>,
    ) -> CommitmentManagerResult<()> {
        for commitment_task_output in commitment_results.into_iter() {
            let height = commitment_task_output.height;
            info!("Writing commitment results to storage for height {}.", height);

            // Decide whether to finalize the block hash based on the config.
            let should_finalize_block_hash = match first_block_with_partial_block_hash.as_ref() {
                Some(FirstBlockWithPartialBlockHash { block_number, .. }) => {
                    height >= *block_number
                }
                None => true,
            };

            // Get the final commitment.
            let FinalBlockCommitment { height, block_hash, global_root } =
                Self::finalize_commitment_output(
                    storage_reader.clone(),
                    commitment_task_output,
                    should_finalize_block_hash,
                )?;

            // Verify the first new block hash matches the configured block hash.
            if let Some(FirstBlockWithPartialBlockHash {
                block_number,
                block_hash: expected_block_hash,
                ..
            }) = first_block_with_partial_block_hash.as_ref()
            {
                if height == *block_number {
                    assert_eq!(
                        *expected_block_hash,
                        block_hash.expect(
                            "The block hash of the first new block should be finalized and \
                             therefore set."
                        ),
                        "The calculated block hash of the first new block ({block_hash:?}) does \
                         not match the configured block hash ({expected_block_hash:?})"
                    );
                }
            }

            // Write the block hash and global root to storage.
            storage_writer.set_global_root_and_block_hash(height, global_root, block_hash)?;
        }

        Ok(())
    }

    /// Writes the ready commitment results to storage.
    pub(crate) async fn get_commitment_results_and_write_to_storage<
        R: BatcherStorageReader + ?Sized,
        W: BatcherStorageWriter + ?Sized,
    >(
        &mut self,
        first_block_with_partial_block_hash: &Option<FirstBlockWithPartialBlockHash>,
        storage_reader: Arc<R>,
        storage_writer: &mut Box<W>,
    ) -> CommitmentManagerResult<()> {
        let commitment_results = self.get_commitment_results().await;
        self.write_commitment_results_to_storage(
            commitment_results,
            first_block_with_partial_block_hash,
            storage_reader.clone(),
            storage_writer,
        )
        .await?;
        Ok(())
    }

    // Private methods.

    fn successfully_added_commitment_task(
        &mut self,
        height: BlockNumber,
        state_diff_commitment: Option<StateDiffCommitment>,
    ) {
        self.task_timer.start_timer(CommitterRequestLabelValue::CommitBlock, height);
        info!(
            "Sent commitment task for block {height} and state diff {state_diff_commitment:?} to \
             state committer."
        );
        self.increase_commitment_task_offset();
    }

    /// Initializes the CommitmentManager. This includes starting the state committer task.
    fn initialize(
        config: &CommitmentManagerConfig,
        global_root_height: BlockNumber,
        committer_client: SharedCommitterClient,
    ) -> Self {
        info!("Initializing CommitmentManager with config {config:?}");
        let (tasks_sender, tasks_receiver) = channel(config.tasks_channel_size);
        let (results_sender, results_receiver) = channel(config.results_channel_size);

        let state_committer = S::create(tasks_receiver, results_sender, committer_client);
        let task_timer = TaskTimer::new();

        Self {
            tasks_sender,
            results_receiver,
            config: config.clone(),
            commitment_task_offset: global_root_height,
            state_committer,
            task_timer,
        }
    }

    fn increase_commitment_task_offset(&mut self) {
        self.commitment_task_offset =
            self.commitment_task_offset.next().expect("Block number overflowed.");
    }

    pub(crate) fn decrease_commitment_task_offset(&mut self) {
        self.commitment_task_offset =
            self.commitment_task_offset.prev().expect("Can't revert before the genesis block.");
    }

    async fn read_commitment_input_and_add_task<
        R: BatcherStorageReader + ?Sized,
        W: BatcherStorageWriter + ?Sized,
    >(
        &mut self,
        height: BlockNumber,
        batcher_storage_reader: Arc<R>,
        batcher_config: &BatcherConfig,
        storage_writer: &mut Box<W>,
    ) {
        let state_diff = match batcher_storage_reader.get_state_diff(height) {
            Ok(Some(diff)) => diff,
            Ok(None) => panic!("Missing state diff for height {height}."),
            Err(err) => panic!("Failed to read state diff for height {height}: {err}"),
        };
        let no_state_diff_commitment = matches!(&batcher_config.first_block_with_partial_block_hash,
            Some(config) if height < config.block_number);

        let state_diff_commitment = if no_state_diff_commitment {
            None
        } else {
            // TODO(Amos): Add method to fetch only hash commitment and use it here.
            match batcher_storage_reader.get_parent_hash_and_partial_block_hash_components(height) {
                Ok((_, Some(PartialBlockHashComponents { header_commitments, .. }))) => {
                    Some(header_commitments.state_diff_commitment)
                }
                Ok((_, None)) => panic!("Missing hash commitment for height {height}."),
                Err(err) => panic!("Failed to read hash commitment for height {height}: {err}"),
            }
        };
        self.add_commitment_task(
            height,
            state_diff,
            state_diff_commitment,
            &batcher_config.first_block_with_partial_block_hash,
            batcher_storage_reader,
            storage_writer,
        )
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
    async fn add_missing_commitment_tasks<
        R: BatcherStorageReader + ?Sized,
        W: BatcherStorageWriter + ?Sized,
    >(
        &mut self,
        current_block_height: BlockNumber,
        batcher_config: &BatcherConfig,
        batcher_storage_reader: Arc<R>,
        storage_writer: &mut Box<W>,
    ) {
        let start = self.get_commitment_task_offset();
        let end = current_block_height;
        for height in start.iter_up_to(end) {
            self.read_commitment_input_and_add_task(
                height,
                batcher_storage_reader.clone(),
                batcher_config,
                storage_writer,
            )
            .await;
        }
        info!("Added missing commitment tasks for blocks [{start}, {end}) to commitment manager.");
    }

    // Associated functions.

    pub(crate) async fn add_revert_task(
        &mut self,
        height: BlockNumber,
        reversed_state_diff: ThinStateDiff,
    ) -> CommitmentManagerResult<()> {
        let expected_height =
            self.commitment_task_offset.prev().expect("Can't revert before the genesis block.");
        if height != expected_height {
            return Err(CommitmentManagerError::WrongRevertTaskHeight {
                expected: expected_height,
                actual: height,
            });
        }
        let revert_task_input =
            CommitterTaskInput::Revert(RevertBlockRequest { height, reversed_state_diff });
        self.add_task_if_channel_not_full(revert_task_input)
            .await
            .expect("Tasks channel should be empty.");
        info!("Sent revert task for block {height}.");
        self.decrease_commitment_task_offset();
        Ok(())
    }

    /// Returns the final commitment output for a given commitment task output.
    /// If `should_finalize_block_hash` is true, finalizes the commitment by calculating the block
    /// hash using the global root, the parent block hash and the partial block hash components.
    /// Otherwise, returns the final commitment with no block hash.
    // TODO(Rotem): Test this function.
    // TODO(Amos): Test blocks [0,10] in OS flow tests.
    fn finalize_commitment_output<R: BatcherStorageReader + ?Sized>(
        storage_reader: Arc<R>,
        CommitmentTaskOutput { response: CommitBlockResponse { global_root }, height }: CommitmentTaskOutput,
        should_finalize_block_hash: bool,
    ) -> CommitmentManagerResult<FinalBlockCommitment> {
        match should_finalize_block_hash {
            false => {
                info!("Finalized commitment for block {height} without calculating block hash.");
                Ok(FinalBlockCommitment { height, block_hash: None, global_root })
            }
            true => {
                info!("Finalizing commitment for block {height} with calculating block hash.");
                let (parent_hash, partial_block_hash_components) =
                    storage_reader.get_parent_hash_and_partial_block_hash_components(height)?;
                let parent_hash = parent_hash.ok_or_else(|| {
                    CommitmentManagerError::MissingBlockHash(height.prev().expect(
                        "For the genesis block, the block hash is constant and should not be \
                         fetched from storage.",
                    ))
                })?;
                let partial_block_hash_components = partial_block_hash_components
                    .ok_or(CommitmentManagerError::MissingPartialBlockHashComponents(height))?;
                let block_hash =
                    calculate_block_hash(&partial_block_hash_components, global_root, parent_hash)?;
                Ok(FinalBlockCommitment { height, block_hash: Some(block_hash), global_root })
            }
        }
    }
}
