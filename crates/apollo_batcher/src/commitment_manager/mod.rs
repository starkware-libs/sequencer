#![allow(dead_code, unused_variables)]

use std::sync::Arc;

use apollo_batcher_config::config::BatcherConfig;
use apollo_committer_types::communication::SharedCommitterClient;
use starknet_api::block::BlockNumber;
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
pub(crate) type ApolloCommitmentManager = CommitmentManager<StateCommitter>;

// TODO(amos): Add to Batcher config.
#[derive(Debug, Clone)]
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
pub(crate) struct CommitmentManager<S: StateCommitterTrait> {
    pub(crate) tasks_sender: Sender<CommitmentTaskInput>,
    pub(crate) results_receiver: Receiver<CommitmentTaskOutput>,
    pub(crate) config: CommitmentManagerConfig,
    pub(crate) commitment_task_offset: BlockNumber,
    state_committer: S,
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
            .expect("Failed to get block hash height from storage.");
        info!("Initializing commitment manager.");
        let mut commitment_manager = CommitmentManager::initialize(
            commitment_manager_config,
            global_root_height,
            committer_client,
        );
        let block_height =
            storage_reader.height().expect("Failed to get block height from storage.");
        commitment_manager
            .add_missing_commitment_tasks(block_height, batcher_config, storage_reader)
            .await;
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
                let (parent_hash, partial_block_hash_components) =
                    storage_reader.get_parent_hash_and_partial_block_hash_components(height)?;
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

#[cfg(test)]
mod tests {
    use std::panic;
    use std::time::Duration;

    use apollo_storage::StorageResult;
    use assert_matches::assert_matches;
    use mockall::predicate::eq;
    use rstest::{fixture, rstest};
    use starknet_api::block::BlockHash;
    use tokio::time::{sleep, timeout};

    use super::*;
    use crate::batcher::{MockBatcherStorageReader, MockBatcherStorageWriter};
    use crate::test_utils::{
        test_state_diff,
        MockClients,
        MockDependencies,
        MockStateCommitter,
        INITIAL_HEIGHT,
    };

    type MockCommitmentManager = CommitmentManager<MockStateCommitter>;

    #[fixture]
    fn mock_dependencies(mock_clients: MockClients) -> MockDependencies {
        MockDependencies {
            storage_reader: MockBatcherStorageReader::new(),
            storage_writer: MockBatcherStorageWriter::new(),
            clients: mock_clients,
            batcher_config: BatcherConfig::default(),
        }
    }

    #[fixture]
    fn mock_clients() -> MockClients {
        MockClients::default()
    }

    fn add_initial_heights(mock_dependencies: &mut MockDependencies) {
        mock_dependencies.storage_reader.expect_height().returning(|| Ok(INITIAL_HEIGHT));
        mock_dependencies
            .storage_reader
            .expect_global_root_height()
            .returning(|| Ok(INITIAL_HEIGHT));
    }

    fn get_dummy_parent_hash_and_partial_block_hash_components(
        height: &BlockNumber,
    ) -> StorageResult<(Option<BlockHash>, Option<PartialBlockHashComponents>)> {
        let mut partial_block_hash_components = PartialBlockHashComponents::default();
        partial_block_hash_components.block_number = *height;
        Ok((Some(BlockHash::default()), Some(partial_block_hash_components)))
    }

    fn get_number_of_tasks_in_sender<T>(sender: &Sender<T>) -> usize {
        sender.max_capacity() - sender.capacity()
    }

    fn get_number_of_tasks_in_receiver<T>(receiver: &Receiver<T>) -> usize {
        receiver.max_capacity() - receiver.capacity()
    }

    async fn create_mock_commitment_manager(
        mock_dependencies: MockDependencies,
    ) -> MockCommitmentManager {
        let commitment_manager_config = CommitmentManagerConfig {
            tasks_channel_size: 1,
            results_channel_size: 1,
            wait_for_tasks_channel: false,
        };
        CommitmentManager::create_commitment_manager(
            &mock_dependencies.batcher_config,
            // TODO(Amos): Use commitment manager config in batcher config, once it's added.
            &commitment_manager_config,
            &mock_dependencies.storage_reader,
            Arc::new(mock_dependencies.clients.committer_client),
        )
        .await
    }

    #[rstest]
    #[tokio::test]
    async fn test_create_commitment_manager(mut mock_dependencies: MockDependencies) {
        add_initial_heights(&mut mock_dependencies);
        let commitment_manager = create_mock_commitment_manager(mock_dependencies).await;

        assert_eq!(
            commitment_manager.get_commitment_task_offset(),
            INITIAL_HEIGHT,
            "Commitment task offset should be equal to initial height."
        );
        assert_eq!(
            get_number_of_tasks_in_sender(&commitment_manager.tasks_sender),
            0,
            "There should be no tasks in the channel."
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_create_commitment_manager_with_missing_tasks(
        mut mock_dependencies: MockDependencies,
    ) {
        let global_root_height = INITIAL_HEIGHT.prev().unwrap();
        mock_dependencies.storage_reader.expect_height().returning(|| Ok(INITIAL_HEIGHT));
        mock_dependencies
            .storage_reader
            .expect_global_root_height()
            .returning(move || Ok(global_root_height));
        mock_dependencies
            .storage_reader
            .expect_get_parent_hash_and_partial_block_hash_components()
            .with(eq(global_root_height))
            .returning(|height| get_dummy_parent_hash_and_partial_block_hash_components(&height));
        mock_dependencies
            .storage_reader
            .expect_get_state_diff()
            .with(eq(global_root_height))
            .returning(|_| Ok(Some(test_state_diff())));

        let commitment_manager = create_mock_commitment_manager(mock_dependencies).await;

        assert_eq!(
            commitment_manager.get_commitment_task_offset(),
            INITIAL_HEIGHT,
            "Commitment task offset should be equal to initial height."
        );
        assert_eq!(
            get_number_of_tasks_in_sender(&commitment_manager.tasks_sender),
            1,
            "There should be one task in the channel."
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_add_commitment_task(mut mock_dependencies: MockDependencies) {
        add_initial_heights(&mut mock_dependencies);
        let state_diff = test_state_diff();
        let state_diff_commitment = Some(StateDiffCommitment::default());

        let mut commitment_manager = create_mock_commitment_manager(mock_dependencies).await;

        // Verify incorrect height results in error.
        let incorrect_height = INITIAL_HEIGHT.next().unwrap();
        let result = commitment_manager
            .add_commitment_task(incorrect_height, state_diff.clone(), state_diff_commitment)
            .await;
        assert_matches!(
            result,
            Err(CommitmentManagerError::WrongTaskHeight { expected, actual, .. })
            if expected == INITIAL_HEIGHT && actual == incorrect_height
        );

        assert_eq!(
            commitment_manager.config.tasks_channel_size, 1,
            "Tasks channel size should be 1 for this test."
        );

        // Verify correct height adds the task successfully.
        commitment_manager
            .add_commitment_task(INITIAL_HEIGHT, state_diff.clone(), state_diff_commitment)
            .await
            .unwrap_or_else(|_| panic!("Failed to add commitment task with correct height."));
        assert_eq!(
            get_number_of_tasks_in_sender(&commitment_manager.tasks_sender),
            1,
            "There should be one task in the channel."
        );

        // Verify adding task when channel is full results in waiting, when config is set.
        commitment_manager.config.wait_for_tasks_channel = true;
        let add_task_future = commitment_manager.add_commitment_task(
            INITIAL_HEIGHT.next().unwrap(),
            state_diff,
            state_diff_commitment,
        );
        let add_task_result = timeout(Duration::from_secs(1), add_task_future).await;
        assert!(
            add_task_result.is_err(),
            "Commitment manager should wait when adding task to full channel, when configured to \
             do so."
        );
    }

    #[rstest]
    #[tokio::test]
    #[should_panic(expected = "Failed to send commitment task to state committer because the \
                               channel is full. Block: 4")]
    async fn test_add_commitment_task_waits(mut mock_dependencies: MockDependencies) {
        add_initial_heights(&mut mock_dependencies);
        let state_diff = test_state_diff();
        let state_diff_commitment = Some(StateDiffCommitment::default());

        let mut commitment_manager = create_mock_commitment_manager(mock_dependencies).await;

        assert_eq!(
            commitment_manager.config.tasks_channel_size, 1,
            "Tasks channel size should be 1 for this test."
        );

        commitment_manager
            .add_commitment_task(INITIAL_HEIGHT, state_diff.clone(), state_diff_commitment)
            .await
            .unwrap_or_else(|_| panic!("Failed to add commitment task with correct height."));

        commitment_manager
            .add_commitment_task(
                INITIAL_HEIGHT.next().unwrap(),
                state_diff.clone(),
                state_diff_commitment,
            )
            .await
            .expect("This call should panic.")
    }

    #[rstest]
    #[tokio::test]
    async fn test_get_commitment_results(mut mock_dependencies: MockDependencies) {
        add_initial_heights(&mut mock_dependencies);
        let state_diff = test_state_diff();
        let state_diff_commitment = Some(StateDiffCommitment::default());

        let commitment_manager_config = CommitmentManagerConfig {
            tasks_channel_size: 2,
            results_channel_size: 2,
            wait_for_tasks_channel: false,
        };
        let mut commitment_manager = MockCommitmentManager::create_commitment_manager(
            &mock_dependencies.batcher_config,
            // TODO(Amos): Use commitment manager config in batcher config, once it's added.
            &commitment_manager_config,
            &mock_dependencies.storage_reader,
            Arc::new(mock_dependencies.clients.committer_client),
        )
        .await;

        // Verify the commitment manager doesn't wait if there are no results.
        let results = commitment_manager.get_commitment_results().await;
        assert!(results.is_empty(), "There should be no commitment results initially.");

        // Add two tasks and simulate their completion by the mock state committer.
        commitment_manager
            .add_commitment_task(INITIAL_HEIGHT, state_diff.clone(), state_diff_commitment)
            .await
            .unwrap();
        commitment_manager
            .add_commitment_task(
                INITIAL_HEIGHT.next().unwrap(),
                state_diff.clone(),
                state_diff_commitment,
            )
            .await
            .unwrap();
        commitment_manager.state_committer.pop_task_and_insert_result().await;
        commitment_manager.state_committer.pop_task_and_insert_result().await;

        let max_n_retries = 3;
        let mut n_retries = 0;
        while get_number_of_tasks_in_receiver(&commitment_manager.results_receiver) < 2 {
            sleep(Duration::from_secs(1)).await;
            n_retries += 1;
            if n_retries >= max_n_retries {
                panic!("Timed out waiting for commitment results after {max_n_retries} retries.");
            }
        }
        let results = commitment_manager.get_commitment_results().await;
        assert_eq!(results.len(), 2, "There should be two commitment results.");
    }
}
