use std::panic;
use std::sync::Arc;
use std::time::Duration;

use apollo_batcher_config::config::{BatcherConfig, CommitmentManagerConfig};
use apollo_committer_types::communication::MockCommitterClient;
use apollo_storage::StorageResult;
use assert_matches::assert_matches;
use mockall::predicate::eq;
use rstest::{fixture, rstest};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::block_hash::block_hash_calculator::PartialBlockHashComponents;
use starknet_api::core::StateDiffCommitment;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::{sleep, timeout};

use crate::batcher::MockBatcherStorageReader;
use crate::commitment_manager::commitment_manager_impl::CommitmentManager;
use crate::commitment_manager::errors::CommitmentManagerError;
use crate::test_utils::{
    test_state_diff,
    MockStateCommitter,
    INITIAL_HEIGHT,
    LATEST_BLOCK_IN_STORAGE,
};

type MockCommitmentManager = CommitmentManager<MockStateCommitter>;

struct MockDependencies {
    pub(crate) storage_reader: MockBatcherStorageReader,
    pub(crate) batcher_config: BatcherConfig,
    pub(crate) committer_client: MockCommitterClient,
}

#[fixture]
fn mock_dependencies() -> MockDependencies {
    let commitment_manager_config = CommitmentManagerConfig {
        tasks_channel_size: 1,
        results_channel_size: 1,
        wait_for_tasks_channel: false,
    };
    let batcher_config = BatcherConfig { commitment_manager_config, ..Default::default() };

    MockDependencies {
        storage_reader: MockBatcherStorageReader::new(),
        batcher_config,
        committer_client: MockCommitterClient::new(),
    }
}

fn add_initial_heights(mock_dependencies: &mut MockDependencies) {
    mock_dependencies.storage_reader.expect_state_diff_height().returning(|| Ok(INITIAL_HEIGHT));
    mock_dependencies.storage_reader.expect_global_root_height().returning(|| Ok(INITIAL_HEIGHT));
}

fn get_dummy_parent_hash_and_partial_block_hash_components(
    height: &BlockNumber,
) -> StorageResult<(Option<BlockHash>, Option<PartialBlockHashComponents>)> {
    let partial_block_hash_components =
        PartialBlockHashComponents { block_number: *height, ..Default::default() };
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
    CommitmentManager::create_commitment_manager(
        &mock_dependencies.batcher_config,
        &mock_dependencies.batcher_config.commitment_manager_config,
        &mock_dependencies.storage_reader,
        Arc::new(mock_dependencies.committer_client),
    )
    .await
}

async fn wait_for_n_results<T>(receiver: &mut Receiver<T>, expected_n_results: usize) {
    let max_n_retries = 3;
    let mut n_retries = 0;
    while get_number_of_tasks_in_receiver(receiver) < expected_n_results {
        sleep(Duration::from_millis(500)).await;
        n_retries += 1;
        if n_retries >= max_n_retries {
            panic!(
                "Timed out waiting for {} results after {} retries.",
                expected_n_results, max_n_retries
            );
        }
    }
}

async fn await_results<T>(receiver: &mut Receiver<T>, expected_n_results: usize) -> Vec<T> {
    wait_for_n_results(receiver, expected_n_results).await;
    let mut results = Vec::new();
    while let Ok(result) = receiver.try_recv() {
        results.push(result);
    }
    assert_eq!(
        results.len(),
        expected_n_results,
        "Number of received results should be equal to expected number of results."
    );
    results
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
// TODO(Einat): Remove ignore when the committer should be enabled.
#[ignore]
#[tokio::test]
async fn test_create_commitment_manager_with_missing_tasks(
    mut mock_dependencies: MockDependencies,
) {
    let global_root_height = INITIAL_HEIGHT.prev().unwrap();
    mock_dependencies.storage_reader.expect_state_diff_height().returning(|| Ok(INITIAL_HEIGHT));
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

    let mut commitment_manager = create_mock_commitment_manager(mock_dependencies).await;

    assert_eq!(commitment_manager.get_commitment_task_offset(), INITIAL_HEIGHT,);
    assert_eq!(get_number_of_tasks_in_sender(&commitment_manager.tasks_sender), 1,);
    commitment_manager.state_committer.pop_task_and_insert_result().await;
    let results = await_results(&mut commitment_manager.results_receiver, 1).await;
    let result = (results.first().unwrap()).clone().expect_commitment();
    assert_eq!(result.height, global_root_height);
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
        Err(CommitmentManagerError::WrongCommitmentTaskHeight { expected, actual, .. })
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
        state_diff.clone(),
        state_diff_commitment,
    );
    let add_task_result = timeout(Duration::from_millis(500), add_task_future).await;
    assert!(
        add_task_result.is_err(),
        "Commitment manager should wait when adding task to full channel, when configured to do \
         so."
    );

    // Verify that after popping a task, adding the task succeeds.
    commitment_manager.state_committer.pop_task_and_insert_result().await;
    commitment_manager
        .add_commitment_task(INITIAL_HEIGHT.next().unwrap(), state_diff, state_diff_commitment)
        .await
        .expect("Failed to add commitment task after freeing up space.");
    assert_eq!(get_number_of_tasks_in_sender(&commitment_manager.tasks_sender), 1,);
}

#[rstest]
#[tokio::test]
async fn test_add_revert_task_wrong_height(mut mock_dependencies: MockDependencies) {
    add_initial_heights(&mut mock_dependencies);

    let mut commitment_manager = create_mock_commitment_manager(mock_dependencies).await;

    // Verify adding a revert task at the wrong height results in an error.
    let err =
        commitment_manager.add_revert_task(INITIAL_HEIGHT, test_state_diff()).await.unwrap_err();
    assert_matches!(err, CommitmentManagerError::WrongRevertTaskHeight { expected, actual, .. } if expected == LATEST_BLOCK_IN_STORAGE && actual == INITIAL_HEIGHT);
}

#[rstest]
#[tokio::test]
#[should_panic(expected = "The channel is full. channel size: 1.")]
async fn test_add_commitment_task_full(mut mock_dependencies: MockDependencies) {
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

    mock_dependencies.batcher_config.commitment_manager_config = CommitmentManagerConfig {
        tasks_channel_size: 2,
        results_channel_size: 2,
        wait_for_tasks_channel: false,
    };
    let mut commitment_manager = create_mock_commitment_manager(mock_dependencies).await;

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

    let results = await_results(&mut commitment_manager.results_receiver, 2).await;
    let first_result = results.first().unwrap().clone().expect_commitment();
    let second_result = results.get(1).unwrap().clone().expect_commitment();
    assert_eq!(first_result.height, INITIAL_HEIGHT,);
    assert_eq!(second_result.height, INITIAL_HEIGHT.next().unwrap(),);
}

/// Adds two commitments and a revert task to the last commit and inserts the results into the
/// channel. Returns the resulted height.
async fn add_commitments_and_revert_tasks(
    commitment_manager: &mut MockCommitmentManager,
    mut height: BlockNumber,
) -> BlockNumber {
    for _ in 0..2 {
        commitment_manager
            .add_commitment_task(height, test_state_diff(), Some(StateDiffCommitment::default()))
            .await
            .unwrap();
        height = height.next().unwrap();
    }
    height = height.prev().unwrap();
    commitment_manager.add_revert_task(height, test_state_diff()).await.unwrap();

    for _ in 0..3 {
        commitment_manager.state_committer.pop_task_and_insert_result().await;
    }
    height
}

#[rstest]
#[tokio::test]
async fn test_wait_for_revert(mut mock_dependencies: MockDependencies) {
    add_initial_heights(&mut mock_dependencies);
    mock_dependencies.batcher_config.commitment_manager_config = CommitmentManagerConfig {
        tasks_channel_size: 5,
        results_channel_size: 5,
        wait_for_tasks_channel: false,
    };
    let mut commitment_manager = create_mock_commitment_manager(mock_dependencies).await;

    let height = add_commitments_and_revert_tasks(&mut commitment_manager, INITIAL_HEIGHT).await;
    let (commitment_results, revert_result) = commitment_manager.wait_for_revert_result().await;
    assert_eq!(commitment_results.len(), 2);
    assert_eq!(revert_result.height, height);
}

#[rstest]
#[tokio::test]
#[should_panic(expected = "Got revert output")]
async fn test_revert_result_at_getting_commitments(mut mock_dependencies: MockDependencies) {
    add_initial_heights(&mut mock_dependencies);
    mock_dependencies.batcher_config.commitment_manager_config = CommitmentManagerConfig {
        tasks_channel_size: 5,
        results_channel_size: 5,
        wait_for_tasks_channel: false,
    };
    let mut commitment_manager = create_mock_commitment_manager(mock_dependencies).await;

    add_commitments_and_revert_tasks(&mut commitment_manager, INITIAL_HEIGHT).await;
    wait_for_n_results(&mut commitment_manager.results_receiver, 3).await;
    commitment_manager.get_commitment_results().await;
}
