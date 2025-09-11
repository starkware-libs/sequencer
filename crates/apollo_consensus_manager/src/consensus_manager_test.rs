use std::sync::Arc;

use apollo_batcher_types::batcher_types::{GetHeightResponse, RevertBlockInput};
use apollo_batcher_types::communication::MockBatcherClient;
use apollo_class_manager_types::EmptyClassManagerClient;
use apollo_config_manager_types::communication::MockConfigManagerClient;
use apollo_consensus_manager_config::config::ConsensusManagerConfig;
use apollo_l1_gas_price_types::MockL1GasPriceProviderClient;
use apollo_reverts::RevertConfig;
use apollo_signature_manager_types::MockSignatureManagerClient;
use apollo_state_sync_types::communication::MockStateSyncClient;
use mockall::predicate::eq;
use starknet_api::block::BlockNumber;
use tokio::time::{timeout, Duration};

use crate::consensus_manager::ConsensusManager;

const BATCHER_HEIGHT: BlockNumber = BlockNumber(10);

#[tokio::test]
async fn revert_batcher_blocks() {
    const REVERT_UP_TO_AND_INCLUDING_HEIGHT: BlockNumber = BlockNumber(7);

    let mut mock_batcher_client = MockBatcherClient::new();
    mock_batcher_client
        .expect_get_height()
        .returning(|| Ok(GetHeightResponse { height: BATCHER_HEIGHT }));

    let expected_revert_heights =
        (REVERT_UP_TO_AND_INCLUDING_HEIGHT.0..BATCHER_HEIGHT.0).rev().map(BlockNumber);
    for height in expected_revert_heights {
        mock_batcher_client
            .expect_revert_block()
            .times(1)
            .with(eq(RevertBlockInput { height }))
            .returning(|_| Ok(()));
    }

    let manager_config = ConsensusManagerConfig {
        revert_config: RevertConfig {
            revert_up_to_and_including: REVERT_UP_TO_AND_INCLUDING_HEIGHT,
            should_revert: true,
        },
        ..Default::default()
    };

    let consensus_manager = ConsensusManager::new(
        manager_config,
        Arc::new(mock_batcher_client),
        Arc::new(MockStateSyncClient::new()),
        Arc::new(EmptyClassManagerClient),
        Arc::new(MockSignatureManagerClient::new()),
        Arc::new(MockConfigManagerClient::new()),
        Arc::new(MockL1GasPriceProviderClient::new()),
    );

    // TODO(Shahak, dvir): try to solve this better (the test will take 100 milliseconds to run).
    timeout(Duration::from_millis(100), consensus_manager.run()).await.unwrap_err();
}

#[tokio::test]
async fn no_reverts_without_config() {
    let mut mock_batcher = MockBatcherClient::new();
    mock_batcher.expect_revert_block().times(0).returning(|_| Ok(()));
    mock_batcher.expect_get_height().returning(|| Ok(GetHeightResponse { height: BlockNumber(0) }));

    let consensus_manager = ConsensusManager::new(
        ConsensusManagerConfig::default(),
        Arc::new(mock_batcher),
        Arc::new(MockStateSyncClient::new()),
        Arc::new(EmptyClassManagerClient),
        Arc::new(MockSignatureManagerClient::new()),
        Arc::new(MockConfigManagerClient::new()),
        Arc::new(MockL1GasPriceProviderClient::new()),
    );

    // TODO(Shahak, dvir): try to solve this better (the test will take 100 milliseconds to run).
    timeout(Duration::from_millis(100), consensus_manager.run()).await.unwrap_err();
}
