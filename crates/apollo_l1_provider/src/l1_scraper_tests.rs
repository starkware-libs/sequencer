use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use alloy::primitives::U256;
use apollo_l1_provider_types::{Event, L1ProviderClient};
use apollo_sequencer_infra::trace_util::configure_tracing;
use apollo_state_sync_types::communication::MockStateSyncClient;
use apollo_state_sync_types::state_sync_types::SyncBlock;
use itertools::Itertools;
use mempool_test_utils::starknet_api_test_utils::DEFAULT_ANVIL_L1_ACCOUNT_ADDRESS;
use papyrus_base_layer::ethereum_base_layer_contract::{
    EthereumBaseLayerConfig,
    EthereumBaseLayerContract,
    Starknet,
};
use papyrus_base_layer::test_utils::{
    anvil_instance_from_config,
    ethereum_base_layer_config_for_anvil,
};
use starknet_api::block::BlockNumber;
use starknet_api::contract_address;
use starknet_api::core::{EntryPointSelector, Nonce};
use starknet_api::executable_transaction::L1HandlerTransaction as ExecutableL1HandlerTransaction;
use starknet_api::hash::StarkHash;
use starknet_api::transaction::fields::{Calldata, Fee};
use starknet_api::transaction::{L1HandlerTransaction, TransactionHasher, TransactionVersion};

use crate::bootstrapper::Bootstrapper;
use crate::l1_provider::L1ProviderBuilder;
use crate::l1_scraper::{L1Scraper, L1ScraperConfig};
use crate::test_utils::FakeL1ProviderClient;
use crate::{event_identifiers_to_track, L1ProviderConfig};

pub fn in_ci() -> bool {
    std::env::var("CI").is_ok()
}

const fn height_add(block_number: BlockNumber, k: u64) -> BlockNumber {
    BlockNumber(block_number.0 + k)
}

// TODO(Gilad): Replace EthereumBaseLayerContract with a mock that has a provider initialized with
// `with_recommended_fillers`, in order to be able to create txs from non-default users.
async fn scraper(
    base_layer_config: EthereumBaseLayerConfig,
) -> (L1Scraper<EthereumBaseLayerContract>, Arc<FakeL1ProviderClient>) {
    let fake_client = Arc::new(FakeL1ProviderClient::default());
    let base_layer = EthereumBaseLayerContract::new(base_layer_config);

    // Deploy a fresh Starknet contract on Anvil from the bytecode in the JSON file.
    Starknet::deploy(base_layer.contract.provider().clone()).await.unwrap();

    let scraper = L1Scraper::new(
        L1ScraperConfig::default(),
        fake_client.clone(),
        base_layer,
        event_identifiers_to_track(),
    )
    .await
    .unwrap();

    (scraper, fake_client)
}

#[tokio::test]
// TODO(Gilad): extract setup stuff into test helpers once more tests are added and patterns emerge.
async fn txs_happy_flow() {
    if !in_ci() {
        return;
    }

    let base_layer_config = ethereum_base_layer_config_for_anvil(None);
    let _anvil = anvil_instance_from_config(&base_layer_config);
    // Setup.
    let (mut scraper, fake_client) = scraper(base_layer_config).await;

    // Test.
    // Scrape multiple events.
    let l2_contract_address = "0x12";
    let l2_entry_point = "0x34";

    let message_to_l2_0 = scraper.base_layer.contract.sendMessageToL2(
        l2_contract_address.parse().unwrap(),
        l2_entry_point.parse().unwrap(),
        vec![U256::from(1_u8), U256::from(2_u8)],
    );
    let message_to_l2_1 = scraper.base_layer.contract.sendMessageToL2(
        l2_contract_address.parse().unwrap(),
        l2_entry_point.parse().unwrap(),
        vec![U256::from(3_u8), U256::from(4_u8)],
    );

    // Send the transactions.
    for msg in &[message_to_l2_0, message_to_l2_1] {
        msg.send().await.unwrap().get_receipt().await.unwrap();
    }

    const EXPECTED_VERSION: TransactionVersion = TransactionVersion(StarkHash::ZERO);
    let expected_internal_l1_tx = L1HandlerTransaction {
        version: EXPECTED_VERSION,
        nonce: Nonce(StarkHash::ZERO),
        contract_address: contract_address!(l2_contract_address),
        entry_point_selector: EntryPointSelector(StarkHash::from_hex_unchecked(l2_entry_point)),
        calldata: Calldata(
            vec![DEFAULT_ANVIL_L1_ACCOUNT_ADDRESS, StarkHash::ONE, StarkHash::from(2)].into(),
        ),
    };
    let tx = ExecutableL1HandlerTransaction {
        tx_hash: expected_internal_l1_tx
            .calculate_transaction_hash(&scraper.config.chain_id, &EXPECTED_VERSION)
            .unwrap(),
        tx: expected_internal_l1_tx,
        paid_fee_on_l1: Fee(0),
    };
    let first_expected_log = Event::L1HandlerTransaction(tx.clone());

    let expected_internal_l1_tx_2 = L1HandlerTransaction {
        nonce: Nonce(StarkHash::ONE),
        calldata: Calldata(
            vec![DEFAULT_ANVIL_L1_ACCOUNT_ADDRESS, StarkHash::from(3), StarkHash::from(4)].into(),
        ),
        ..tx.tx
    };
    let second_expected_log = Event::L1HandlerTransaction(ExecutableL1HandlerTransaction {
        tx_hash: expected_internal_l1_tx_2
            .calculate_transaction_hash(&scraper.config.chain_id, &EXPECTED_VERSION)
            .unwrap(),
        tx: expected_internal_l1_tx_2,
        ..tx
    });

    // Assert.
    scraper.send_events_to_l1_provider().await.unwrap();
    fake_client.assert_add_events_received_with(&[first_expected_log, second_expected_log]);

    // Previous events had been scraped, should no longer appear.
    scraper.send_events_to_l1_provider().await.unwrap();
    fake_client.assert_add_events_received_with(&[]);
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
    sync_client
        .expect_get_block()
        .returning(move |input| Ok(sync_response_clone.lock().unwrap().remove(&input)));
    sync_client.expect_get_latest_block_number().returning(move || Ok(Some(CATCH_UP_HEIGHT)));
    let config = L1ProviderConfig {
        startup_sync_sleep_retry_interval: Duration::from_millis(10),
        ..Default::default()
    };
    let mut l1_provider =
        L1ProviderBuilder::new(config, l1_provider_client.clone(), Arc::new(sync_client))
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
    tokio::time::sleep(config.startup_sync_sleep_retry_interval).await;

    // **Commit** 2 blocks past catchup height, should be received after the previous sync.
    let no_txs_committed = vec![]; // Not testing txs in this test.
    l1_provider_client
        .commit_block(no_txs_committed.clone(), height_add(CATCH_UP_HEIGHT, 1))
        .await
        .unwrap();
    tokio::time::sleep(config.startup_sync_sleep_retry_interval).await;
    l1_provider_client
        .commit_block(no_txs_committed, height_add(CATCH_UP_HEIGHT, 2))
        .await
        .unwrap();
    tokio::time::sleep(config.startup_sync_sleep_retry_interval).await;

    // Feed sync task the remaining blocks, will be received after the commits above.
    sync_response.lock().unwrap().insert(height_add(STARTUP_HEIGHT, 1), SyncBlock::default());
    sync_response.lock().unwrap().insert(height_add(STARTUP_HEIGHT, 2), SyncBlock::default());
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

#[tokio::test]
async fn bootstrap_delayed_sync_state_with_trivial_catch_up() {
    if !in_ci() {
        return;
    }
    configure_tracing().await;

    // Setup.

    let l1_provider_client = Arc::new(FakeL1ProviderClient::default());
    const STARTUP_HEIGHT: BlockNumber = BlockNumber(3);

    let mut sync_client = MockStateSyncClient::default();
    // Mock sync response for an arbitrary number of calls to get_latest_block_number.
    // Later in the test we modify it to become something else.
    let sync_height_response = Arc::new(Mutex::new(None));
    let sync_response_clone = sync_height_response.clone();
    sync_client
        .expect_get_latest_block_number()
        .returning(move || Ok(*sync_response_clone.lock().unwrap()));

    let config = L1ProviderConfig {
        startup_sync_sleep_retry_interval: Duration::from_millis(10),
        ..Default::default()
    };
    let mut l1_provider =
        L1ProviderBuilder::new(config, l1_provider_client.clone(), Arc::new(sync_client))
            .startup_height(STARTUP_HEIGHT)
            .build();

    // Test.

    // Start the sync sequence, should busy-wait until the sync height is sent.
    let scraped_l1_handler_txs = []; // No txs to scrape in this test.
    l1_provider.initialize(scraped_l1_handler_txs.into()).await.unwrap();

    // **Commit** a few blocks. The height starts from the provider's current height, since this
    // is a trivial catchup scenario (nothing to catch up).
    // This checks that the trivial catch_up_height doesn't mess up this flow.
    let no_txs_committed = []; // Not testing txs in this test.
    l1_provider_client.commit_block(no_txs_committed.to_vec(), STARTUP_HEIGHT).await.unwrap();
    l1_provider_client
        .commit_block(no_txs_committed.to_vec(), height_add(STARTUP_HEIGHT, 1))
        .await
        .unwrap();

    // Forward all messages buffered in the client to the provider.
    l1_provider_client.flush_messages(&mut l1_provider).await;

    // Commit blocks should have been applied.
    let start_height_plus_2 = height_add(STARTUP_HEIGHT, 2);
    assert_eq!(l1_provider.current_height, start_height_plus_2);
    // Should still be bootstrapping, since catchup height isn't determined yet.
    // Technically we could end bootstrapping at this point, but its simpler to let it
    // terminate gracefully once the the sync is ready.
    assert!(l1_provider.state.is_bootstrapping());

    *sync_height_response.lock().unwrap() = Some(BlockNumber(2));

    // Let the sync task continue, it should short circuit.
    tokio::time::sleep(config.startup_sync_sleep_retry_interval).await;
    // Assert height is unchanged from last time, no commit block was called from the sync task.
    assert_eq!(l1_provider.current_height, start_height_plus_2);
    // Finally, commit a new block to trigger the bootstrapping check, should switch to steady
    // state.
    l1_provider.commit_block(&no_txs_committed, start_height_plus_2).unwrap();
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
    let sync_height = BlockNumber(3);

    let mut sync_client = MockStateSyncClient::default();
    // Mock sync response for an arbitrary number of calls to get_latest_block_number.
    // Later in the test we modify it to become something else.
    let sync_height_response = Arc::new(Mutex::new(None));
    let sync_response_clone = sync_height_response.clone();
    sync_client
        .expect_get_latest_block_number()
        .returning(move || Ok(*sync_response_clone.lock().unwrap()));
    sync_client.expect_get_block().returning(|_| Ok(Some(SyncBlock::default())));

    let config = L1ProviderConfig {
        startup_sync_sleep_retry_interval: Duration::from_millis(10),
        ..Default::default()
    };
    let mut l1_provider =
        L1ProviderBuilder::new(config, l1_provider_client.clone(), Arc::new(sync_client))
            .startup_height(startup_height)
            .build();

    // Test.

    // Start the sync sequence, should busy-wait until the sync height is sent.
    let scraped_l1_handler_txs = []; // No txs to scrape in this test.
    l1_provider.initialize(scraped_l1_handler_txs.into()).await.unwrap();

    // **Commit** a few blocks. These should get backlogged since they are post-sync-height.
    // Sleeps are sprinkled in to give the async task a couple shots at attempting to get the sync
    // height (see DEBUG log).
    let no_txs_committed = []; // Not testing txs in this test.
    l1_provider_client
        .commit_block(no_txs_committed.to_vec(), sync_height.unchecked_next())
        .await
        .unwrap();
    tokio::time::sleep(config.startup_sync_sleep_retry_interval).await;
    l1_provider_client
        .commit_block(no_txs_committed.to_vec(), sync_height.unchecked_next().unchecked_next())
        .await
        .unwrap();

    // Forward all messages buffered in the client to the provider.
    l1_provider_client.flush_messages(&mut l1_provider).await;
    tokio::time::sleep(config.startup_sync_sleep_retry_interval).await;

    // Assert commit blocks are backlogged (didn't affect start height).
    assert_eq!(l1_provider.current_height, startup_height);
    // Should still be bootstrapping, since catchup height isn't determined yet.
    assert!(l1_provider.state.is_bootstrapping());

    // Simulate the state sync service finally being ready, and give the async task enough time to
    // pick this up and sync up the provider.
    *sync_height_response.lock().unwrap() = Some(sync_height);
    tokio::time::sleep(config.startup_sync_sleep_retry_interval).await;
    // Forward all messages buffered in the client to the provider.
    l1_provider_client.flush_messages(&mut l1_provider).await;

    // Two things happened here: the async task sent 2 commit blocks it got from the sync_client,
    // which bumped the provider height to sync_height+1, then the backlog was applied which bumped
    // it twice again.
    assert_eq!(
        l1_provider.current_height,
        sync_height.unchecked_next().unchecked_next().unchecked_next()
    );
    // Sync height was reached, bootstrapping was completed.
    assert!(!l1_provider.state.is_bootstrapping());
}

#[tokio::test]
#[should_panic = "Sync task is stuck"]
async fn test_stuck_sync() {
    configure_tracing().await;
    const STARTUP_HEIGHT: BlockNumber = BlockNumber(1);

    let mut sync_client = MockStateSyncClient::default();
    sync_client.expect_get_latest_block_number().once().returning(|| panic!("CRASH the sync task"));

    let l1_provider_client = Arc::new(FakeL1ProviderClient::default());
    let config = Default::default();
    let mut l1_provider =
        L1ProviderBuilder::new(config, l1_provider_client.clone(), Arc::new(sync_client))
            .startup_height(STARTUP_HEIGHT)
            .build();

    // Test.

    // Start sync.
    l1_provider.initialize(Default::default()).await.unwrap();

    for i in 0..=(Bootstrapper::MAX_HEALTH_CHECK_FAILURES + 1) {
        l1_provider.commit_block(&[], height_add(STARTUP_HEIGHT, i.into())).unwrap();
        tokio::time::sleep(config.startup_sync_sleep_retry_interval).await;
    }
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
