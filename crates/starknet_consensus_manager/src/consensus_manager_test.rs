use std::sync::Arc;

use mockall::predicate::eq;
use papyrus_consensus_orchestrator::cende::CendeConfig;
use rstest::rstest;
use starknet_api::block::BlockNumber;
use starknet_batcher_types::batcher_types::{GetHeightResponse, RevertBlockInput};
use starknet_batcher_types::communication::MockBatcherClient;
use starknet_state_sync_types::communication::MockStateSyncClient;

use crate::config::ConsensusManagerConfig;
use crate::consensus_manager::ConsensusManager;

#[rstest]
#[case::no_skip_write_height(None, 0)]
#[case::should_revert(Some(BlockNumber(9)), 1)]
#[case::batcher_smaller_skip_height(Some(BlockNumber(8)), 0)]
#[case::batcher_eq_skip_height(Some(BlockNumber(10)), 0)]
#[case::batcher_greater_skip_height(Some(BlockNumber(11)), 0)]
#[tokio::test]
async fn revert_if_needed(
    #[case] skip_write_height: Option<BlockNumber>,
    #[case] revert_call_count: usize,
) {
    const BATCHER_HEIGHT: BlockNumber = BlockNumber(10);

    let mut mock_batcher_client = MockBatcherClient::new();
    mock_batcher_client
        .expect_get_height()
        .returning(|| Ok(GetHeightResponse { height: BATCHER_HEIGHT }));
    mock_batcher_client
        .expect_revert_block()
        .times(revert_call_count)
        .with(eq(RevertBlockInput { height: BATCHER_HEIGHT.prev().unwrap() }))
        .returning(|_| Ok(()));

    let cende_config = CendeConfig { skip_write_height, ..Default::default() };
    let manager_config = ConsensusManagerConfig { cende_config, ..Default::default() };

    let consensus_manager = ConsensusManager::new(
        manager_config,
        Arc::new(mock_batcher_client),
        Arc::new(MockStateSyncClient::new()),
    );

    consensus_manager.revert_if_needed().await;
}
