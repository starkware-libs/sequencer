use std::sync::Arc;
use std::time::Duration;

use mockall::predicate::eq;
use starknet_api::block::BlockNumber;
use starknet_batcher_types::batcher_types::{GetHeightResponse, RevertBlockInput};
use starknet_batcher_types::communication::MockBatcherClient;
use starknet_state_sync_types::communication::MockStateSyncClient;
use tokio::time::timeout;

use crate::config::ConsensusManagerConfig;
use crate::consensus_manager::ConsensusManager;

const BATCHER_HEIGHT: BlockNumber = BlockNumber(10);

#[tokio::test]
async fn revert_batcher_blocks_if_needed() {
    const REVERT_FROM_HEIGHT: BlockNumber = BlockNumber(7);

    let mut mock_batcher_client = MockBatcherClient::new();
    mock_batcher_client
        .expect_get_height()
        .returning(|| Ok(GetHeightResponse { height: BATCHER_HEIGHT }));

    let expected_revert_heights = [BlockNumber(9), BlockNumber(8), BlockNumber(7)];
    for height in expected_revert_heights.iter() {
        mock_batcher_client
            .expect_revert_block()
            .times(1)
            .with(eq(RevertBlockInput { height: *height }))
            .returning(|_| Ok(()));
    }

    let manager_config = ConsensusManagerConfig {
        revert_up_to_and_including: Some(REVERT_FROM_HEIGHT),
        ..Default::default()
    };

    let consensus_manager = ConsensusManager::new(
        manager_config,
        Arc::new(mock_batcher_client),
        Arc::new(MockStateSyncClient::new()),
    );

    timeout(Duration::from_millis(1500), consensus_manager.revert_batcher_blocks_if_needed())
        .await
        .expect_err("The function should enter an eternal loop");
}

#[should_panic(expected = "Batcher height marker 10 is not larger than the target height marker \
                           10. No reverts are needed.")]
#[tokio::test]
async fn try_revert_from_equal_block() {
    let mut mock_batcher = MockBatcherClient::new();
    mock_batcher.expect_get_height().returning(|| Ok(GetHeightResponse { height: BATCHER_HEIGHT }));

    let manager_config = ConsensusManagerConfig {
        revert_up_to_and_including: Some(BATCHER_HEIGHT),
        ..Default::default()
    };

    let consensus_manager = ConsensusManager::new(
        manager_config,
        Arc::new(mock_batcher),
        Arc::new(MockStateSyncClient::new()),
    );

    consensus_manager.revert_batcher_blocks_if_needed().await;
}

#[should_panic(expected = "Batcher height marker 10 is not larger than the target height marker \
                           11. No reverts are needed.")]
#[tokio::test]
async fn try_revert_from_larger_block() {
    let mut mock_batcher = MockBatcherClient::new();
    mock_batcher.expect_get_height().returning(|| Ok(GetHeightResponse { height: BATCHER_HEIGHT }));

    let manager_config = ConsensusManagerConfig {
        revert_up_to_and_including: BATCHER_HEIGHT.next(),
        ..Default::default()
    };

    let consensus_manager = ConsensusManager::new(
        manager_config,
        Arc::new(mock_batcher),
        Arc::new(MockStateSyncClient::new()),
    );

    consensus_manager.revert_batcher_blocks_if_needed().await;
}

#[tokio::test]
async fn no_reverts_without_config() {
    let manager_config =
        ConsensusManagerConfig { revert_up_to_and_including: None, ..Default::default() };

    let consensus_manager = ConsensusManager::new(
        manager_config,
        Arc::new(MockBatcherClient::new()),
        Arc::new(MockStateSyncClient::new()),
    );

    consensus_manager.revert_batcher_blocks_if_needed().await;
}
