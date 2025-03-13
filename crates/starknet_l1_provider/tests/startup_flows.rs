use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use itertools::Itertools;
use mempool_test_utils::in_ci;
use starknet_api::block::BlockNumber;
use starknet_l1_provider::l1_provider::create_l1_provider;
use starknet_l1_provider::test_utils::FakeL1ProviderClient;
use starknet_l1_provider::L1ProviderConfig;
use starknet_l1_provider_types::L1ProviderClient;
use starknet_sequencer_infra::trace_util::configure_tracing;
use starknet_state_sync_types::communication::MockStateSyncClient;
use starknet_state_sync_types::state_sync_types::SyncBlock;

// TODO(Gilad): figure out how To setup anvil on a specific L1 block (through genesis.json?) and
// with a specified L2 block logged to L1 (hopefully without having to use real backup).
/// This test simulates a bootstrapping flow, in which 3 blocks are synced from L2, during which two
/// new blocks from past the catch-up height arrive. The expected behavior is that the synced
/// commit_blocks are processed as they come, and the two new blocks are backlogged until the synced
/// blocks are processed, after which they are processed in order.
#[tokio::test]
async fn bootstrap_e2e() {
    if !in_ci() {
        return;
    }
    configure_tracing().await;

    // Setup.

    let l1_provider_client = Arc::new(FakeL1ProviderClient::default());
    let startup_height = BlockNumber(2);
    let catch_up_height = BlockNumber(5);

    // Make the mocked sync client try removing from a hashmap as a response to get block.
    let mut sync_client = MockStateSyncClient::default();
    let sync_response = Arc::new(Mutex::new(HashMap::<BlockNumber, SyncBlock>::new()));
    let mut sync_response_clone = sync_response.lock().unwrap().clone();
    sync_client.expect_get_block().returning(move |input| Ok(sync_response_clone.remove(&input)));

    let config = L1ProviderConfig {
        bootstrap_catch_up_height: catch_up_height,
        startup_sync_sleep_retry_interval: Duration::from_millis(10),
        ..Default::default()
    };
    let mut l1_provider = create_l1_provider(
        config,
        l1_provider_client.clone(),
        Arc::new(sync_client),
        startup_height,
    );

    // Test.

    // Trigger the bootstrapper: this will trigger the sync task to start trying to fetch blocks
    // from the sync client, which will always return nothing since the hash map above is still
    // empty. The sync task will busy-wait on the height until we feed the hashmap.
    // TODO(Gilad): Consider adding txs here and in the commit blocks, might make the test harder to
    // understand though.
    let scraped_l1_handler_txs = vec![]; // No txs to scrape in this test.
    l1_provider.initialize(scraped_l1_handler_txs).await.unwrap();

    // Load first **Sync** response: the initializer task will pick it up within the specified
    // interval.
    sync_response.lock().unwrap().insert(startup_height, SyncBlock::default());
    tokio::time::sleep(config.startup_sync_sleep_retry_interval).await;

    // **Commit** 2 blocks past catchup height, should be received after the previous sync.
    let no_txs_committed = vec![]; // Not testing txs in this test.
    l1_provider_client.commit_block(no_txs_committed.clone(), catch_up_height).await.unwrap();
    tokio::time::sleep(config.startup_sync_sleep_retry_interval).await;
    l1_provider_client
        .commit_block(no_txs_committed, catch_up_height.unchecked_next())
        .await
        .unwrap();
    tokio::time::sleep(config.startup_sync_sleep_retry_interval).await;

    // Feed sync task the remaining blocks, will be received after the commits above.
    sync_response.lock().unwrap().insert(BlockNumber(startup_height.0 + 1), SyncBlock::default());
    sync_response.lock().unwrap().insert(BlockNumber(startup_height.0 + 2), SyncBlock::default());
    tokio::time::sleep(2 * config.startup_sync_sleep_retry_interval).await;

    // Assert that initializer task has received the stubbed responses from the sync client and sent
    // the corresponding commit blocks to the provider, in the order implied to by the test
    // structure.
    let mut commit_blocks = l1_provider_client.commit_blocks_received.lock().unwrap();
    let received_order = commit_blocks.iter().map(|block| block.height).collect_vec();
    let expected_order =
        vec![BlockNumber(2), BlockNumber(5), BlockNumber(6), BlockNumber(3), BlockNumber(4)];
    assert_eq!(
        received_order, expected_order,
        "Sanity check failed: commit block order mismatch. Expected {:?}, got {:?}",
        expected_order, received_order
    );

    // Apply commit blocks and assert that correct height commit_blocks are applied, but commit
    // blocks past catch_up_height are backlogged.
    // TODO(Gilad): once we are able to create clients on top of channels, this manual'ness won't
    // be necessary. Right now we cannot create clients without spinning up all servers, so we have
    // to use a mock.

    let mut commit_blocks = commit_blocks.drain(..);

    // Apply height 2.
    let next_block = commit_blocks.next().unwrap();
    l1_provider.commit_block(&next_block.committed_txs, next_block.height).unwrap();
    assert_eq!(l1_provider.current_height, BlockNumber(3));

    // Backlog height 5.
    let next_block = commit_blocks.next().unwrap();
    l1_provider.commit_block(&next_block.committed_txs, next_block.height).unwrap();
    // Assert that this didn't affect height; this commit block is too high so is backlogged.
    assert_eq!(l1_provider.current_height, BlockNumber(3));

    // Backlog height 6.
    let next_block = commit_blocks.next().unwrap();
    l1_provider.commit_block(&next_block.committed_txs, next_block.height).unwrap();
    // Assert backlogged, like height 5.
    assert_eq!(l1_provider.current_height, BlockNumber(3));

    // Apply height 3
    let next_block = commit_blocks.next().unwrap();
    l1_provider.commit_block(&next_block.committed_txs, next_block.height).unwrap();
    assert_eq!(l1_provider.current_height, BlockNumber(4));

    // Apply height 4 ==> this triggers committing the backlogged heights 5 and 6.
    let next_block = commit_blocks.next().unwrap();
    l1_provider.commit_block(&next_block.committed_txs, next_block.height).unwrap();
    assert_eq!(l1_provider.current_height, BlockNumber(7));

    // Assert that the bootstrapper has been dropped.
    assert!(!l1_provider.state.is_bootstrapping());
}

#[test]
#[ignore = "similar to backlog_happy_flow, only shorter, and sprinkle some start_block/get_txs \
            attempts while its bootstrapping (and assert failure on height), then assert that they \
            succeed after bootstrapping ends."]
fn bootstrap_completion() {
    todo!()
}
