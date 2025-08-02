use std::sync::Arc;

use apollo_batcher_types::communication::MockBatcherClient;
use apollo_l1_provider::config::L1ProviderConfig;
use apollo_l1_provider::l1_provider::L1ProviderBuilder;
use apollo_l1_provider::ProviderState;
use apollo_l1_provider_types::MockL1ProviderClient;
use apollo_state_sync_types::communication::MockStateSyncClient;
use apollo_time::test_utils::FakeClock;
use pretty_assertions::assert_eq;
use starknet_api::block::BlockNumber;
use starknet_api::tx_hash;

#[tokio::test]
async fn reexecution_flow_historical_blocks_ignored() {
    // Setup: Provider starts at height 5, but catch-up height is 3 (2 blocks _behind_)
    let start_height = BlockNumber(5);
    let catch_up_height = BlockNumber(3);
    let mut l1_provider = L1ProviderBuilder::new(
        L1ProviderConfig::default(),
        Arc::new(MockL1ProviderClient::default()),
        Arc::new(MockBatcherClient::default()),
        Arc::new(MockStateSyncClient::default()),
    )
    .startup_height(start_height)
    .catchup_height(catch_up_height)
    .clock(Arc::new(FakeClock::new(0)))
    .build();

    // Initialize the provider
    l1_provider.initialize(vec![]).await.unwrap();

    let unchanged_l1_provider = l1_provider.clone();
    for historical_height in catch_up_height.iter_up_to(start_height) {
        let arbitrary_unknown_tx_hashes = [tx_hash!(1), tx_hash!(2)];
        l1_provider
            .commit_block(arbitrary_unknown_tx_hashes.into(), [].into(), historical_height)
            .unwrap();

        // Verify the provider state is unchanged
        assert_eq!(l1_provider, unchanged_l1_provider);
    }

    // Test: Commit block with correct height (5) should bump the height
    l1_provider.commit_block([].into(), [].into(), start_height).unwrap();

    assert_eq!(l1_provider.current_height, start_height.unchecked_next());
    assert_eq!(l1_provider.state, ProviderState::Pending);
}
