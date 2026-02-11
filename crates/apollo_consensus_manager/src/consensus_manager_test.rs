use std::sync::{Arc, Mutex};

use apollo_batcher_types::batcher_types::{GetHeightResponse, RevertBlockInput};
use apollo_batcher_types::communication::MockBatcherClient;
use apollo_class_manager_types::MockClassManagerClient;
use apollo_config_manager_types::communication::MockConfigManagerClient;
use apollo_consensus::storage::MockHeightVotedStorageTrait;
use apollo_consensus::test_utils::get_new_storage_config;
use apollo_consensus_config::config::{ConsensusConfig, ConsensusStaticConfig};
use apollo_consensus_manager_config::config::ConsensusManagerConfig;
use apollo_l1_gas_price_types::MockL1GasPriceProviderClient;
use apollo_reverts::RevertConfig;
use apollo_signature_manager_types::MockSignatureManagerClient;
use apollo_state_sync_types::communication::MockStateSyncClient;
use mockall::predicate::eq;
use mockall::Sequence;
use starknet_api::block::BlockNumber;
use tokio::time::{timeout, Duration};

use crate::consensus_manager::{ConsensusManager, ConsensusManagerArgs};

#[tokio::test]
async fn revert_batcher_blocks() {
    const BATCHER_HEIGHT: BlockNumber = BlockNumber(10);
    const REVERT_UP_TO_AND_INCLUDING_HEIGHT: BlockNumber = BlockNumber(7);

    let mut revert_sequence = Sequence::new();

    let mut mock_batcher_client = MockBatcherClient::new();
    mock_batcher_client
        .expect_get_height()
        .returning(|| Ok(GetHeightResponse { height: BATCHER_HEIGHT }));

    let mut mock_voted_height_storage = MockHeightVotedStorageTrait::new();

    mock_voted_height_storage
        .expect_revert_height()
        .times(1)
        .with(eq(REVERT_UP_TO_AND_INCLUDING_HEIGHT))
        .returning(|_| Ok(()));

    let expected_revert_heights =
        (REVERT_UP_TO_AND_INCLUDING_HEIGHT.0..BATCHER_HEIGHT.0).rev().map(BlockNumber);
    for height in expected_revert_heights {
        mock_batcher_client
            .expect_revert_block()
            .times(1)
            .with(eq(RevertBlockInput { height }))
            .in_sequence(&mut revert_sequence)
            .returning(|_| Ok(()));
    }

    let manager_config = ConsensusManagerConfig {
        revert_config: RevertConfig {
            revert_up_to_and_including: REVERT_UP_TO_AND_INCLUDING_HEIGHT,
            should_revert: true,
        },
        ..Default::default()
    };

    let consensus_manager = ConsensusManager::new_with_storage(
        ConsensusManagerArgs {
            config: manager_config,
            batcher_client: Arc::new(mock_batcher_client),
            state_sync_client: Arc::new(MockStateSyncClient::new()),
            class_manager_client: Arc::new(MockClassManagerClient::new()),
            signature_manager_client: Arc::new(MockSignatureManagerClient::new()),
            config_manager_client: Arc::new(MockConfigManagerClient::new()),
            l1_gas_price_provider: Arc::new(MockL1GasPriceProviderClient::new()),
        },
        Arc::new(Mutex::new(mock_voted_height_storage)),
    );

    // TODO(Shahak): try to solve this better (the test will take 100 milliseconds to run).
    timeout(Duration::from_millis(100), consensus_manager.run()).await.unwrap_err();
}

#[tokio::test]
async fn revert_voted_height_when_batcher_already_at_target() {
    const TARGET_HEIGHT: BlockNumber = BlockNumber(7);

    let mut mock_batcher_client = MockBatcherClient::new();
    // Batcher is already at the target height — no blocks to revert.
    mock_batcher_client
        .expect_get_height()
        .returning(|| Ok(GetHeightResponse { height: TARGET_HEIGHT }));
    mock_batcher_client.expect_revert_block().times(0).returning(|_| Ok(()));

    let mut mock_voted_height_storage = MockHeightVotedStorageTrait::new();
    // Checking we're still reverting the consensus manager's voted height storage.
    mock_voted_height_storage
        .expect_revert_height()
        .times(1)
        .with(eq(TARGET_HEIGHT))
        .returning(|_| Ok(()));

    let manager_config = ConsensusManagerConfig {
        revert_config: RevertConfig {
            revert_up_to_and_including: TARGET_HEIGHT,
            should_revert: true,
        },
        ..Default::default()
    };

    let consensus_manager = ConsensusManager::new_with_storage(
        ConsensusManagerArgs {
            config: manager_config,
            batcher_client: Arc::new(mock_batcher_client),
            state_sync_client: Arc::new(MockStateSyncClient::new()),
            class_manager_client: Arc::new(MockClassManagerClient::new()),
            signature_manager_client: Arc::new(MockSignatureManagerClient::new()),
            config_manager_client: Arc::new(MockConfigManagerClient::new()),
            l1_gas_price_provider: Arc::new(MockL1GasPriceProviderClient::new()),
        },
        Arc::new(Mutex::new(mock_voted_height_storage)),
    );

    // TODO(Shahak): try to solve this better (the test will take 100 milliseconds to run).
    timeout(Duration::from_millis(100), consensus_manager.run()).await.unwrap_err();
}

#[tokio::test]
async fn no_reverts_without_config() {
    let mut mock_batcher = MockBatcherClient::new();
    mock_batcher.expect_revert_block().times(0).returning(|_| Ok(()));
    mock_batcher.expect_get_height().returning(|| Ok(GetHeightResponse { height: BlockNumber(0) }));

    let consensus_manager = ConsensusManager::new(ConsensusManagerArgs {
        config: ConsensusManagerConfig {
            consensus_manager_config: ConsensusConfig {
                static_config: ConsensusStaticConfig {
                    storage_config: get_new_storage_config(),
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        },
        batcher_client: Arc::new(mock_batcher),
        state_sync_client: Arc::new(MockStateSyncClient::new()),
        class_manager_client: Arc::new(MockClassManagerClient::new()),
        signature_manager_client: Arc::new(MockSignatureManagerClient::new()),
        config_manager_client: Arc::new(MockConfigManagerClient::new()),
        l1_gas_price_provider: Arc::new(MockL1GasPriceProviderClient::new()),
    });

    // TODO(Shahak): try to solve this better (the test will take 100 milliseconds to run).
    timeout(Duration::from_millis(100), consensus_manager.run()).await.unwrap_err();
}
