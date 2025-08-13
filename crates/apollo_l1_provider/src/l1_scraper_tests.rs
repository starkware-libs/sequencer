use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use apollo_batcher_types::batcher_types::GetHeightResponse;
use apollo_batcher_types::communication::MockBatcherClient;
use apollo_infra::trace_util::configure_tracing;
use apollo_l1_provider_types::errors::L1ProviderError;
use apollo_l1_provider_types::{L1ProviderClient, MockL1ProviderClient};
use apollo_state_sync_types::communication::MockStateSyncClient;
use apollo_state_sync_types::errors::StateSyncError;
use apollo_state_sync_types::state_sync_types::SyncBlock;
use assert_matches::assert_matches;
use indexmap::IndexSet;
use itertools::Itertools;
<<<<<<< HEAD
use papyrus_base_layer::{L1BlockReference, MockBaseLayerContract};
||||||| 38f03e1d0
use papyrus_base_layer::ethereum_base_layer_contract::{
    EthereumBaseLayerConfig,
    EthereumBaseLayerContract,
    Starknet,
};
use papyrus_base_layer::test_utils::{
    anvil_instance_from_config,
    ethereum_base_layer_config_for_anvil,
};
use papyrus_base_layer::{BaseLayerContract, L1BlockReference, MockBaseLayerContract};
=======
use papyrus_base_layer::ethereum_base_layer_contract::{
    EthereumBaseLayerConfig,
    EthereumBaseLayerContract,
    Starknet,
};
use papyrus_base_layer::test_utils::{
    anvil_instance_from_config,
    ethereum_base_layer_config_for_anvil,
};
use papyrus_base_layer::{BaseLayerContract, L1BlockHash, L1BlockReference, MockBaseLayerContract};
>>>>>>> origin/main-v0.14.0
use rstest::{fixture, rstest};
use starknet_api::block::BlockNumber;
use starknet_api::transaction::TransactionHash;

use crate::bootstrapper::Bootstrapper;
use crate::l1_provider::{L1Provider, L1ProviderBuilder};
use crate::l1_scraper::{L1Scraper, L1ScraperConfig, L1ScraperError};
use crate::test_utils::FakeL1ProviderClient;
use crate::{event_identifiers_to_track, L1ProviderConfig};

pub fn in_ci() -> bool {
    std::env::var("CI").is_ok()
}

const fn height_add(block_number: BlockNumber, k: u64) -> BlockNumber {
    BlockNumber(block_number.0 + k)
}

// Can't mock clients in runtime (mockall not applicable), hence mocking sender and receiver.
async fn send_commit_block(
    l1_provider_client: &FakeL1ProviderClient,
    committed: &[TransactionHash],
    height: BlockNumber,
) {
    l1_provider_client
        .commit_block((committed).iter().copied().collect(), [].into(), height)
        .await
        .unwrap();
}

// Can't mock clients in runtime (mockall not applicable), hence mocking sender and receiver.
fn receive_commit_block(
    l1_provider: &mut L1Provider,
    committed: &IndexSet<TransactionHash>,
    height: BlockNumber,
) {
    l1_provider.commit_block(committed.iter().copied().collect(), [].into(), height).unwrap();
}

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
    const STARTUP_HEIGHT: BlockNumber = BlockNumber(2);
    const CATCH_UP_HEIGHT: BlockNumber = BlockNumber(4);

    // Make the mocked sync client try removing from a hashmap as a response to get block.
    let mut sync_client = MockStateSyncClient::default();
    let sync_response = Arc::new(Mutex::new(HashMap::<BlockNumber, SyncBlock>::new()));
    let sync_response_clone = sync_response.clone();
    sync_client.expect_get_block().returning(move |input| {
        sync_response_clone
            .lock()
            .unwrap()
            .remove(&input)
            .ok_or(StateSyncError::BlockNotFound(input).into())
    });

    let mut batcher_client = MockBatcherClient::default();
    batcher_client
        .expect_get_height()
        .returning(move || Ok(GetHeightResponse { height: CATCH_UP_HEIGHT.unchecked_next() }));

    let config = L1ProviderConfig {
        startup_sync_sleep_retry_interval_seconds: Duration::from_millis(10),
        ..Default::default()
    };
    let mut l1_provider = L1ProviderBuilder::new(
        config,
        l1_provider_client.clone(),
        Arc::new(batcher_client),
        Arc::new(sync_client),
    )
    .startup_height(STARTUP_HEIGHT)
    .build();

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
    sync_response.lock().unwrap().insert(STARTUP_HEIGHT, SyncBlock::default());
    tokio::time::sleep(config.startup_sync_sleep_retry_interval_seconds).await;

    // **Commit** 2 blocks past catchup height, should be received after the previous sync.
    let no_txs_committed = vec![]; // Not testing txs in this test.

    send_commit_block(&l1_provider_client, &no_txs_committed, height_add(CATCH_UP_HEIGHT, 1)).await;
    tokio::time::sleep(config.startup_sync_sleep_retry_interval_seconds).await;
    send_commit_block(&l1_provider_client, &no_txs_committed, height_add(CATCH_UP_HEIGHT, 2)).await;
    tokio::time::sleep(config.startup_sync_sleep_retry_interval_seconds).await;

    // Feed sync task the remaining blocks, will be received after the commits above.
    sync_response.lock().unwrap().insert(height_add(STARTUP_HEIGHT, 1), SyncBlock::default());
    sync_response.lock().unwrap().insert(height_add(STARTUP_HEIGHT, 2), SyncBlock::default());
    tokio::time::sleep(2 * config.startup_sync_sleep_retry_interval_seconds).await;

    // Assert that initializer task has received the stubbed responses from the sync client and sent
    // the corresponding commit blocks to the provider, in the order implied to by the test
    // structure.
    let mut commit_blocks = l1_provider_client.commit_blocks_received.lock().unwrap();
    let received_order = commit_blocks.iter().map(|block| block.height).collect_vec();
    let expected_order =
        vec![BlockNumber(2), BlockNumber(5), BlockNumber(6), BlockNumber(3), BlockNumber(4)];
    assert_eq!(
        received_order, expected_order,
        "Sanity check failed: commit block order mismatch. Expected {expected_order:?}, got \
         {received_order:?}"
    );

    // Apply commit blocks and assert that correct height commit_blocks are applied, but commit
    // blocks past catch_up_height are backlogged.
    // TODO(Gilad): once we are able to create clients on top of channels, this manual'ness won't
    // be necessary. Right now we cannot create clients without spinning up all servers, so we have
    // to use a mock.

    let mut commit_blocks = commit_blocks.drain(..);

    // Apply height 2.
    let next_block = commit_blocks.next().unwrap();
    receive_commit_block(&mut l1_provider, &next_block.committed_txs, next_block.height);
    assert_eq!(l1_provider.current_height, BlockNumber(3));

    // Backlog height 5.
    let next_block = commit_blocks.next().unwrap();
    receive_commit_block(&mut l1_provider, &next_block.committed_txs, next_block.height);
    // Assert that this didn't affect height; this commit block is too high so is backlogged.
    assert_eq!(l1_provider.current_height, BlockNumber(3));

    // Backlog height 6.
    let next_block = commit_blocks.next().unwrap();
    receive_commit_block(&mut l1_provider, &next_block.committed_txs, next_block.height);
    // Assert backlogged, like height 5.
    assert_eq!(l1_provider.current_height, BlockNumber(3));

    // Apply height 3
    let next_block = commit_blocks.next().unwrap();
    receive_commit_block(&mut l1_provider, &next_block.committed_txs, next_block.height);
    assert_eq!(l1_provider.current_height, BlockNumber(4));

    // Apply height 4 ==> this triggers committing the backlogged heights 5 and 6.
    let next_block = commit_blocks.next().unwrap();
    receive_commit_block(&mut l1_provider, &next_block.committed_txs, next_block.height);
    assert_eq!(l1_provider.current_height, BlockNumber(7));

    // Assert that the bootstrapper has been dropped.
    assert!(!l1_provider.state.is_bootstrapping());
}

#[tokio::test]
async fn bootstrap_delayed_batcher_and_sync_state_with_trivial_catch_up() {
    if !in_ci() {
        return;
    }
    configure_tracing().await;

    // Setup.

    let l1_provider_client = Arc::new(FakeL1ProviderClient::default());
    const STARTUP_HEIGHT: BlockNumber = BlockNumber(3);

    let mut batcher_client = MockBatcherClient::default();
    let batcher_response_height = Arc::new(Mutex::new(BlockNumber(0)));
    let batcher_response_height_clone = batcher_response_height.clone();
    batcher_client.expect_get_height().returning(move || {
        Ok(GetHeightResponse { height: *batcher_response_height_clone.lock().unwrap() })
    });

    let sync_client = MockStateSyncClient::default();
    let config = L1ProviderConfig {
        startup_sync_sleep_retry_interval_seconds: Duration::from_millis(10),
        ..Default::default()
    };
    let mut l1_provider = L1ProviderBuilder::new(
        config,
        l1_provider_client.clone(),
        Arc::new(batcher_client),
        Arc::new(sync_client),
    )
    .startup_height(STARTUP_HEIGHT)
    .build();

    // Test.

    // Start the sync sequence, should busy-wait until the batcher height is sent.
    let scraped_l1_handler_txs = []; // No txs to scrape in this test.
    l1_provider.initialize(scraped_l1_handler_txs.into()).await.unwrap();

    // **Commit** a few blocks. The height starts from the provider's current height, since this
    // is a trivial catchup scenario (nothing to catch up).
    // This checks that the trivial catch_up_height doesn't mess up this flow.
    let no_txs_committed = []; // Not testing txs in this test.
    send_commit_block(&l1_provider_client, &no_txs_committed, STARTUP_HEIGHT).await;
    send_commit_block(&l1_provider_client, &no_txs_committed, height_add(STARTUP_HEIGHT, 1)).await;

    // Forward all messages buffered in the client to the provider.
    l1_provider_client.flush_messages(&mut l1_provider).await;

    // Commit blocks should have been applied.
    let start_height_plus_2 = height_add(STARTUP_HEIGHT, 2);
    assert_eq!(l1_provider.current_height, start_height_plus_2);
    // Should still be bootstrapping, since catchup height isn't determined yet.
    // Technically we could end bootstrapping at this point, but its simpler to let it
    // terminate gracefully once the batcher and sync are ready.
    assert!(l1_provider.state.is_bootstrapping());

    *batcher_response_height.lock().unwrap() = STARTUP_HEIGHT;
    // Let the sync task continue, it should short circuit.
    tokio::time::sleep(config.startup_sync_sleep_retry_interval_seconds).await;
    // Assert height is unchanged from last time, no commit block was called from the sync task.
    assert_eq!(l1_provider.current_height, start_height_plus_2);
    // Finally, commit a new block to trigger the bootstrapping check, should switch to steady
    // state.
    receive_commit_block(&mut l1_provider, &no_txs_committed.into(), start_height_plus_2);
    assert_eq!(l1_provider.current_height, height_add(start_height_plus_2, 1));
    // The new commit block triggered the catch-up check, which ended the bootstrapping phase.
    assert!(!l1_provider.state.is_bootstrapping());
}

#[tokio::test]
async fn bootstrap_delayed_sync_state_with_sync_behind_batcher() {
    if !in_ci() {
        return;
    }
    configure_tracing().await;

    // Setup.

    let l1_provider_client = Arc::new(FakeL1ProviderClient::default());
    let startup_height = BlockNumber(1);
    let batcher_height = BlockNumber(4);

    let mut sync_client = MockStateSyncClient::default();
    // Mock sync response for an arbitrary number of calls to get block.
    // Later in the test we modify it to become something else.
    let sync_block_response = Arc::new(Mutex::new(HashMap::<BlockNumber, SyncBlock>::new()));
    let sync_response_clone = sync_block_response.clone();
    sync_client.expect_get_block().returning(move |input| {
        sync_response_clone
            .lock()
            .unwrap()
            .remove(&input)
            .ok_or(StateSyncError::BlockNotFound(input).into())
    });

    let mut batcher_client = MockBatcherClient::default();
    batcher_client
        .expect_get_height()
        .returning(move || Ok(GetHeightResponse { height: batcher_height }));

    let config = L1ProviderConfig {
        startup_sync_sleep_retry_interval_seconds: Duration::from_millis(10),
        ..Default::default()
    };
    let mut l1_provider = L1ProviderBuilder::new(
        config,
        l1_provider_client.clone(),
        Arc::new(batcher_client),
        Arc::new(sync_client),
    )
    .startup_height(startup_height)
    .build();

    // Test.

    // Start the sync sequence, should busy-wait until the sync blocks are sent.
    let scraped_l1_handler_txs = []; // No txs to scrape in this test.
    l1_provider.initialize(scraped_l1_handler_txs.into()).await.unwrap();

    // **Commit** a few blocks. These should get backlogged since they are post-sync-height.
    // Sleeps are sprinkled in to give the async task time to get the batcher height and have a
    // couple shots at attempting to get the sync blocks (see DEBUG log).
    let no_txs_committed = []; // Not testing txs in this test.
    send_commit_block(&l1_provider_client, &no_txs_committed, batcher_height).await;
    tokio::time::sleep(config.startup_sync_sleep_retry_interval_seconds).await;
    send_commit_block(&l1_provider_client, &no_txs_committed, batcher_height.unchecked_next())
        .await;

    // Forward all messages buffered in the client to the provider.
    l1_provider_client.flush_messages(&mut l1_provider).await;
    tokio::time::sleep(config.startup_sync_sleep_retry_interval_seconds).await;

    // Assert commit blocks are backlogged (didn't affect start height).
    assert_eq!(l1_provider.current_height, startup_height);
    // Should still be bootstrapping, since sync hasn't caught up to the batcher height yet.
    assert!(l1_provider.state.is_bootstrapping());

    // Simulate the state sync service finally being ready, and give the async task enough time to
    // pick this up and sync up the provider.
    sync_block_response.lock().unwrap().insert(startup_height, SyncBlock::default());
    sync_block_response
        .lock()
        .unwrap()
        .insert(startup_height.unchecked_next(), SyncBlock::default());
    sync_block_response
        .lock()
        .unwrap()
        .insert(startup_height.unchecked_next().unchecked_next(), SyncBlock::default());
    tokio::time::sleep(config.startup_sync_sleep_retry_interval_seconds).await;
    // Forward all messages buffered in the client to the provider.
    l1_provider_client.flush_messages(&mut l1_provider).await;

    // Two things happened here: the async task sent 2 commit blocks it got from the sync_client,
    // which bumped the provider height to batcher_height, then the backlog was applied which
    // bumped it twice again.
    assert_eq!(l1_provider.current_height, batcher_height.unchecked_next().unchecked_next());
    // Batcher height was reached, bootstrapping was completed.
    assert!(!l1_provider.state.is_bootstrapping());
}

#[tokio::test]
#[should_panic = "Sync task is stuck"]
async fn test_stuck_sync() {
    configure_tracing().await;
    const STARTUP_HEIGHT: BlockNumber = BlockNumber(1);

    let mut batcher_client = MockBatcherClient::default();
    batcher_client.expect_get_height().once().returning(|| panic!("CRASH the sync task"));

    let sync_client = MockStateSyncClient::default();
    let l1_provider_client = Arc::new(FakeL1ProviderClient::default());
    let config = L1ProviderConfig {
        // Override the default retry interval which is way too long for a test.
        startup_sync_sleep_retry_interval_seconds: Duration::from_millis(10),
        ..Default::default()
    };
    let mut l1_provider = L1ProviderBuilder::new(
        config,
        l1_provider_client.clone(),
        Arc::new(batcher_client),
        Arc::new(sync_client),
    )
    .startup_height(STARTUP_HEIGHT)
    .build();

    // Test.

    // Start sync.
    l1_provider.initialize(Default::default()).await.unwrap();

    for i in 0..=(Bootstrapper::MAX_HEALTH_CHECK_FAILURES + 1) {
        receive_commit_block(&mut l1_provider, &[].into(), height_add(STARTUP_HEIGHT, i.into()));
        tokio::time::sleep(config.startup_sync_sleep_retry_interval_seconds).await;
    }
}

#[rstest]
#[tokio::test]
/// If the provider crashes, the scraper should detect this and trigger a self restart by returning
/// an appropriate error.
async fn provider_crash_should_crash_scraper(mut dummy_base_layer: MockBaseLayerContract) {
    // Setup.
    let mut l1_provider_client = MockL1ProviderClient::default();
    l1_provider_client.expect_add_events().once().returning(|_| {
        Err(apollo_l1_provider_types::errors::L1ProviderClientError::L1ProviderError(
            L1ProviderError::Uninitialized,
        ))
    });
    dummy_base_layer.expect_l1_block_at().returning(|_| Ok(Some(Default::default())));

    let mut scraper = L1Scraper::new(
        L1ScraperConfig::default(),
        Arc::new(l1_provider_client),
        dummy_base_layer,
        event_identifiers_to_track(),
        L1BlockReference::default(),
    )
    .await
    .unwrap();

    // Test.
    assert_eq!(scraper.send_events_to_l1_provider().await, Err(L1ScraperError::NeedsRestart));
}

#[fixture]
fn dummy_base_layer() -> MockBaseLayerContract {
    let mut base_layer = MockBaseLayerContract::new();
    base_layer.expect_latest_l1_block_number().return_once(|_| Ok(Some(Default::default())));
    base_layer.expect_latest_l1_block().return_once(|_| Ok(Some(Default::default())));
    base_layer.expect_events().return_once(|_, _| Ok(Default::default()));
    base_layer
}

#[rstest]
#[tokio::test]
async fn l1_reorg_block_hash(mut dummy_base_layer: MockBaseLayerContract) {
    // Setup.
    let mut l1_provider_client = MockL1ProviderClient::default();

    let l1_block_at_response = Arc::new(Mutex::new(Some(Default::default())));
    let l1_block_at_response_clone = l1_block_at_response.clone();
    dummy_base_layer
        .expect_l1_block_at()
        .returning(move |_| Ok(*l1_block_at_response_clone.lock().unwrap()));

    l1_provider_client.expect_add_events().times(1).returning(|_| Ok(()));
    let mut scraper = L1Scraper::new(
        L1ScraperConfig::default(),
        Arc::new(l1_provider_client),
        dummy_base_layer,
        event_identifiers_to_track(),
        L1BlockReference::default(),
    )
    .await
    .unwrap();

    // Test.
    // Can send messages to the provider.
    assert_eq!(scraper.send_events_to_l1_provider().await, Ok(()));

    // Simulate an L1 fork: last block hash changed due to reorg.
    let l1_block_hash_after_l1_reorg = L1BlockHash([123; 32]);
    *l1_block_at_response.lock().unwrap() =
        Some(L1BlockReference { hash: l1_block_hash_after_l1_reorg, ..Default::default() });

    assert_matches!(
        scraper.send_events_to_l1_provider().await,
        Err(L1ScraperError::L1ReorgDetected { .. })
    );
}

#[rstest]
#[tokio::test]
async fn l1_reorg_block_number(mut dummy_base_layer: MockBaseLayerContract) {
    // Setup.
    let mut l1_provider_client = MockL1ProviderClient::default();
    l1_provider_client.expect_add_events().returning(|_| Ok(()));

    let l1_block_at_response = Arc::new(Mutex::new(Some(Default::default())));
    let l1_block_at_response_clone = l1_block_at_response.clone();
    dummy_base_layer
        .expect_l1_block_at()
        .returning(move |_| Ok(*l1_block_at_response_clone.lock().unwrap()));

    let mut scraper = L1Scraper::new(
        L1ScraperConfig::default(),
        Arc::new(l1_provider_client),
        dummy_base_layer,
        event_identifiers_to_track(),
        L1BlockReference::default(),
    )
    .await
    .unwrap();

    // Test.
    // can send messages to the provider.
    assert_eq!(scraper.send_events_to_l1_provider().await, Ok(()));

    // Simulate an L1 revert: the last processed l1 block no longer exists.
    *l1_block_at_response.lock().unwrap() = None;

    assert_matches!(
        scraper.send_events_to_l1_provider().await,
        Err(L1ScraperError::L1ReorgDetected { .. })
    );
}

#[test]
#[ignore = "similar to backlog_happy_flow, only shorter, and sprinkle some start_block/get_txs \
            attempts while its bootstrapping (and assert failure on height), then assert that they \
            succeed after bootstrapping ends."]
fn bootstrap_completion() {
    todo!()
}

#[tokio::test]
#[ignore = "Not yet implemented: generate an l1 and an cancel event for that tx, also check an \
            abort for a different tx"]
async fn cancel_l1_handlers() {}

#[tokio::test]
#[ignore = "Not yet implemented: check that when the scraper resets all txs from the last T time
are processed"]
async fn reset() {}

#[tokio::test]
#[ignore = "Not yet implemented: check successful consume."]
async fn consume() {}
