use apollo_batcher_config::config::BatcherConfig;
use starknet_api::block::BlockNumber;
use starknet_api::block_hash::block_hash_calculator::PartialBlockHashComponents;
use tracing::info;

use crate::batcher::BatcherStorageReader;
use crate::commitment_manager::{CommitmentManager, CommitmentManagerConfig};

/// Adds missing commitment tasks to the commitment manager. Missing tasks are caused by unfinished
/// commitment tasks / results not written to storage when the sequencer is shut down.
pub(crate) async fn add_missing_commitment_tasks(
    current_block_height: BlockNumber,
    batcher_config: &BatcherConfig,
    commitment_manager: &mut CommitmentManager,
    batcher_storage_reader: &impl BatcherStorageReader,
) {
    let (start, end) =
        commitment_manager.get_missing_commitment_tasks_heights(current_block_height);
    for height in start.iter_up_to(end) {
        let state_diff = match batcher_storage_reader.get_state_diff(height) {
            Ok(Some(diff)) => diff,
            Ok(None) => panic!("Missing state diff for height {height}."),
            Err(err) => panic!("Failed to read state diff for height {height}: {}", err),
        };
        let state_diff_commitment = if current_block_height
            < batcher_config.first_block_with_partial_block_hash.block_number
        {
            None
        } else {
            match batcher_storage_reader.get_partial_block_hash_components(height) {
                Ok(Some(PartialBlockHashComponents { header_commitments, .. })) => {
                    Some(header_commitments.state_diff_commitment)
                }
                Ok(None) => panic!("Missing hash commitment for height {height}."),
                Err(err) => panic!("Failed to read hash commitment for height {height}: {}", err),
            }
        };
        commitment_manager.add_commitment_task(height, state_diff, state_diff_commitment).await;
    }
    info!("Added missing commitment tasks for blocks [{start}, {end}) to commitment manager.");
}

/// If not in revert mode - creates and initializes the commitment manager, and also adds missing
/// commitment tasks. Otherwise, returns None.
pub(crate) async fn create_commitment_manager_or_none(
    batcher_config: &BatcherConfig,
    commitment_manager_config: &CommitmentManagerConfig,
    storage_reader: &impl BatcherStorageReader,
) -> Option<CommitmentManager> {
    let block_hash_height =
        storage_reader.block_hash_height().expect("Failed to get block hash height from storage.");
    let mut commitment_manager = CommitmentManager::new_or_none(
        commitment_manager_config,
        &batcher_config.revert_config,
        block_hash_height,
    );
    if let Some(ref mut cm) = commitment_manager {
        let block_height =
            storage_reader.height().expect("Failed to get block height from storage.");
        add_missing_commitment_tasks(block_height, batcher_config, cm, storage_reader).await;
    };
    commitment_manager
}
