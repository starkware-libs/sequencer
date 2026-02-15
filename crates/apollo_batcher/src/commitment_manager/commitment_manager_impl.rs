#![allow(dead_code, unused_variables)]

use std::sync::Arc;

use apollo_batcher_config::config::{BatcherConfig, CommitmentManagerConfig};
use apollo_committer_types::committer_types::{CommitBlockRequest, CommitBlockResponse};
use apollo_committer_types::communication::{CommitterRequest, SharedCommitterClient};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::block_hash::block_hash_calculator::{
    calculate_block_hash,
    PartialBlockHashComponents,
};
use starknet_api::core::StateDiffCommitment;
use starknet_api::state::ThinStateDiff;
use tokio::sync::mpsc::error::{TryRecvError, TrySendError};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tracing::info;

use crate::batcher::BatcherStorageReader;
use crate::commitment_manager::errors::CommitmentManagerError;
use crate::commitment_manager::state_committer::{StateCommitter, StateCommitterTrait};
use crate::commitment_manager::types::{
    CommitmentTaskOutput,
    CommitterTaskInput,
    CommitterTaskOutput,
    FinalBlockCommitment,
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
}

impl<S: StateCommitterTrait> CommitmentManager<S> {
    // Public methods.

    /// Creates and initializes the commitment manager, and also adds
    /// missing commitment tasks.
    pub(crate) async fn create_commitment_manager<R: BatcherStorageReader>(
        batcher_config: &BatcherConfig,
        commitment_manager_config: &CommitmentManagerConfig,
        storage_reader: &R,
        committer_client: SharedCommitterClient,
    ) -> Self {
        let global_root_height = storage_reader
            .global_root_height()
            .expect("Failed to get global root height from storage.");
        info!("Initializing commitment manager.");
        let commitment_manager = CommitmentManager::initialize(
            commitment_manager_config,
            global_root_height,
            committer_client,
        );
        let block_height =
            storage_reader.state_diff_height().expect("Failed to get block height from storage.");
        // TODO(Einat): Uncomment when the committer should be enabled.
        // commitment_manager
        //     .add_missing_commitment_tasks(block_height, batcher_config, storage_reader)
        //     .await;
        commitment_manager
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
            CommitterTaskInput(CommitterRequest::CommitBlock(CommitBlockRequest {
                height,
                state_diff,
                state_diff_commitment,
            }));
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

    /// Fetches all ready commitment results from the state committer. Panics if any task is a
    /// revert.
    pub(crate) async fn get_commitment_results(&mut self) -> Vec<CommitmentTaskOutput> {
        let mut results = Vec::new();
        loop {
            match self.results_receiver.try_recv() {
                Ok(result) => results.push(result.expect_commitment()),
                Err(TryRecvError::Empty) => break,
                Err(err) => {
                    panic!("Failed to receive commitment result from state committer. error: {err}")
                }
            }
        }
        results
    }

    // Private methods.

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

        Self {
            tasks_sender,
            results_receiver,
            config: config.clone(),
            commitment_task_offset: global_root_height,
            state_committer,
        }
    }

    fn increase_commitment_task_offset(&mut self) {
        self.commitment_task_offset =
            self.commitment_task_offset.next().expect("Block number overflowed.");
    }

    async fn read_commitment_input_and_add_task<R: BatcherStorageReader>(
        &mut self,
        height: BlockNumber,
        batcher_storage_reader: &R,
        batcher_config: &BatcherConfig,
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
        self.add_commitment_task(height, state_diff, state_diff_commitment).await.unwrap();
        info!(
            "Added commitment task for block {height}, {state_diff_commitment:?} to commitment \
             manager."
        );
    }

    /// Adds missing commitment tasks to the commitment manager. Missing tasks are caused by
    /// unfinished commitment tasks / results not written to storage when the sequencer is shut
    /// down.
    async fn add_missing_commitment_tasks<R: BatcherStorageReader>(
        &mut self,
        current_block_height: BlockNumber,
        batcher_config: &BatcherConfig,
        batcher_storage_reader: &R,
    ) {
        let start = self.get_commitment_task_offset();
        let end = current_block_height;
        for height in start.iter_up_to(end) {
            self.read_commitment_input_and_add_task(height, batcher_storage_reader, batcher_config)
                .await;
        }
        info!("Added missing commitment tasks for blocks [{start}, {end}) to commitment manager.");
    }

    // Associated functions.

    pub(crate) async fn revert_block(height: BlockNumber, reversed_state_diff: ThinStateDiff) {
        unimplemented!()
    }

    /// Returns the final commitment output for a given commitment task output.
    /// If `should_finalize_block_hash` is true, finalizes the commitment by calculating the block
    /// hash using the global root, the parent block hash and the partial block hash components.
    /// Otherwise, returns the final commitment with no block hash.
    // TODO(Rotem): Test this function.
    // TODO(Amos): Test blocks [0,10] in OS flow tests.
    pub(crate) fn final_commitment_output<R: BatcherStorageReader + ?Sized>(
        storage_reader: Arc<R>,
        CommitmentTaskOutput { response: CommitBlockResponse { state_root: global_root }, height }: CommitmentTaskOutput,
        should_finalize_block_hash: bool,
    ) -> CommitmentManagerResult<FinalBlockCommitment> {
        match should_finalize_block_hash {
            false => {
                info!("Finalized commitment for block {height} without calculating block hash.");
                Ok(FinalBlockCommitment { height, block_hash: None, global_root })
            }
            true => {
                info!("Finalizing commitment for block {height} with calculating block hash.");
                let (mut parent_hash, partial_block_hash_components) =
                    storage_reader.get_parent_hash_and_partial_block_hash_components(height)?;
                if height == BlockNumber::ZERO {
                    parent_hash = Some(BlockHash::GENESIS_PARENT_HASH);
                }
                let parent_hash = parent_hash.ok_or(CommitmentManagerError::MissingBlockHash(
                    height.prev().expect(
                        "For the genesis block, the block hash is constant and should not be \
                         fetched from storage.",
                    ),
                ))?;
                let partial_block_hash_components = partial_block_hash_components
                    .ok_or(CommitmentManagerError::MissingPartialBlockHashComponents(height))?;
                let block_hash =
                    calculate_block_hash(&partial_block_hash_components, global_root, parent_hash)?;
                Ok(FinalBlockCommitment { height, block_hash: Some(block_hash), global_root })
            }
        }
    }
}
