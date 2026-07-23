use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use apollo_l1_events_config::config::L1EventsScraperConfig;
use apollo_l1_events_types::errors::L1EventsProviderError;
use apollo_l1_events_types::{Event, MockL1EventsProviderClient};
use assert_matches::assert_matches;
use papyrus_base_layer::{
    L1BlockHash,
    L1BlockReference,
    L1Event,
    MockBaseLayerContract,
    MockError,
};
use starknet_api::block::BlockTimestamp;
use starknet_api::transaction::fields::{Calldata, Fee};
use starknet_api::transaction::L1HandlerTransaction;
use starknet_types_core::felt::Felt;

use crate::event_identifiers_to_track;
use crate::l1_scraper::{L1EventsScraper, L1EventsScraperError};

fn dummy_base_layer() -> MockBaseLayerContract {
    let mut base_layer = MockBaseLayerContract::new();
    base_layer.expect_latest_l1_block_number().return_once(|| Ok(Default::default()));
    base_layer.expect_events().return_once(|_, _| Ok(Default::default()));
    base_layer
}

async fn scraper_with_dummy() -> L1EventsScraper<MockBaseLayerContract> {
    let base_layer = dummy_base_layer();
    let mut l1_events_provider_client = MockL1EventsProviderClient::default();
    l1_events_provider_client.expect_add_events().returning(|_| Ok(()));

    let mut scraper = L1EventsScraper::new(
        L1EventsScraperConfig::default(),
        Arc::new(l1_events_provider_client),
        base_layer,
        event_identifiers_to_track(),
    )
    .await
    .unwrap();

    // Skipping scraper run loop, instead must give it a start block.
    scraper.scrape_from_this_l1_block = Some(Default::default());
    scraper
}

#[tokio::test]
/// If the provider crashes, the scraper should detect this and trigger a self restart by returning
/// an appropriate error.
async fn provider_crash_should_crash_scraper() {
    // Setup.
    let mut l1_events_provider_client = MockL1EventsProviderClient::default();
    l1_events_provider_client.expect_add_events().once().returning(|_| {
        Err(apollo_l1_events_types::errors::L1EventsProviderClientError::L1EventsProviderError(
            L1EventsProviderError::Uninitialized,
        ))
    });
    let mut scraper = scraper_with_dummy().await;
    scraper.l1_events_provider_client = Arc::new(l1_events_provider_client);
    scraper.base_layer.expect_l1_block_at().returning(|_| Ok(Some(Default::default())));

    // Test.
    assert_eq!(
        scraper.send_events_to_l1_events_provider().await,
        Err(L1EventsScraperError::NeedsRestart)
    );
}

#[tokio::test]
async fn l1_reorg_block_hash() {
    // Setup.
    let mut scraper = scraper_with_dummy().await;
    let l1_block_at_response = Arc::new(Mutex::new(Some(Default::default())));
    let l1_block_at_response_clone = l1_block_at_response.clone();

    scraper
        .base_layer
        .expect_l1_block_at()
        .returning(move |_| Ok(*l1_block_at_response_clone.lock().unwrap()));

    // Test.
    // Can send messages to the provider.
    assert_eq!(scraper.send_events_to_l1_events_provider().await, Ok(()));

    // Simulate an L1 fork: last block hash changed due to reorg.
    let l1_block_hash_after_l1_reorg = L1BlockHash([123; 32]);
    *l1_block_at_response.lock().unwrap() =
        Some(L1BlockReference { hash: l1_block_hash_after_l1_reorg, ..Default::default() });

    assert_matches!(
        scraper.send_events_to_l1_events_provider().await,
        Err(L1EventsScraperError::L1ReorgDetected { .. })
    );
}

#[tokio::test]
async fn l1_reorg_block_number() {
    // Setup.
    let mut scraper = scraper_with_dummy().await;

    let l1_block_at_response = Arc::new(Mutex::new(Some(Default::default())));
    let l1_block_at_response_clone = l1_block_at_response.clone();
    scraper
        .base_layer
        .expect_l1_block_at()
        .returning(move |_| Ok(*l1_block_at_response_clone.lock().unwrap()));

    // Test.
    // can send messages to the provider.
    assert_eq!(scraper.send_events_to_l1_events_provider().await, Ok(()));

    // Simulate an L1 revert: the last processed l1 block no longer exists.
    *l1_block_at_response.lock().unwrap() = None;

    assert_matches!(
        scraper.send_events_to_l1_events_provider().await,
        Err(L1EventsScraperError::L1ReorgDetected { .. })
    );
}

#[tokio::test]
async fn latest_block_number_goes_down() {
    // Setup.
    const L1_LATEST_BLOCK_NUMBER: u64 = 10;
    const L1_BLOCK_HASH: L1BlockHash = L1BlockHash([123; 32]);
    const L1_BAD_LATEST_NUMBER: u64 = 5;

    let mut dummy_base_layer: MockBaseLayerContract = MockBaseLayerContract::new();

    // This should always be returned, even if we set the "response" to a lower block number.
    let expected_block_reference =
        L1BlockReference { number: L1_LATEST_BLOCK_NUMBER, hash: L1_BLOCK_HASH };

    let initial_block_reference = L1BlockReference { number: 0, hash: L1_BLOCK_HASH };

    let latest_l1_block_number_response = Arc::new(AtomicU64::new(L1_LATEST_BLOCK_NUMBER));
    let latest_l1_block_number_response_clone = latest_l1_block_number_response.clone();

    dummy_base_layer
        .expect_latest_l1_block_number()
        .times(2)
        .returning(move || Ok(latest_l1_block_number_response_clone.load(Ordering::Relaxed)));

    dummy_base_layer.expect_events().times(1).returning(|_, _| Ok(vec![]));

    dummy_base_layer
        .expect_l1_block_at()
        .returning(move |number| Ok(Some(L1BlockReference { number, hash: L1_BLOCK_HASH })));

    let mut scraper = scraper_with_dummy().await;
    scraper.scrape_from_this_l1_block = Some(initial_block_reference);
    scraper.base_layer = dummy_base_layer;

    // Test.
    // Can send messages to the provider.
    // This should also set the scraper's last_l1_block_processed to block number 10.
    assert_eq!(scraper.send_events_to_l1_events_provider().await, Ok(()));

    // Simulate a base layer returning a lower block number.
    latest_l1_block_number_response.store(L1_BAD_LATEST_NUMBER, Ordering::Relaxed);

    // Make sure we don't hit the reorg error in this scenario.
    scraper.assert_no_l1_reorgs().await.unwrap();

    // Should ignore and try again on the next interval, returning the same block reference.
    assert_eq!(scraper.fetch_events().await, Ok((expected_block_reference, vec![])));
}

#[tokio::test]
async fn base_layer_returns_block_number_below_finality_causes_error() {
    // Setup.
    const FINALITY: u64 = 10;
    const INITIAL_L1_BLOCK_NUMBER: u64 = 100;
    const L1_BLOCK_HASH: L1BlockHash = L1BlockHash([123; 32]);
    const WRONG_L1_BLOCK_NUMBER: u64 = 5;

    let initial_block_reference =
        L1BlockReference { number: INITIAL_L1_BLOCK_NUMBER, hash: L1_BLOCK_HASH };

    let latest_l1_block_number_response = Arc::new(AtomicU64::new(INITIAL_L1_BLOCK_NUMBER));
    let latest_l1_block_number_response_clone = latest_l1_block_number_response.clone();

    let mut dummy_base_layer: MockBaseLayerContract = MockBaseLayerContract::new();

    dummy_base_layer
        .expect_latest_l1_block_number()
        .returning(move || Ok(latest_l1_block_number_response_clone.load(Ordering::Relaxed)));
    dummy_base_layer
        .expect_l1_block_at()
        .returning(move |number| Ok(Some(L1BlockReference { number, hash: L1_BLOCK_HASH })));

    let mut scraper = scraper_with_dummy().await;
    scraper.config.finality = FINALITY;

    scraper.scrape_from_this_l1_block = Some(initial_block_reference);
    scraper.base_layer = dummy_base_layer;

    // Test.
    scraper.send_events_to_l1_events_provider().await.unwrap();

    // Simulate a base layer returning a lower block number.
    latest_l1_block_number_response.store(WRONG_L1_BLOCK_NUMBER, Ordering::Relaxed);

    // The scraper should return a finality too high error.
    assert_matches!(
        scraper.send_events_to_l1_events_provider().await,
        Err(L1EventsScraperError::LatestBlockNumberTooLow { .. })
    );
}

// A single fetch must never request more than max_blocks_per_fetch blocks, even when latest is far
// ahead, and the cursor must advance by exactly one window (not jump to latest).
#[tokio::test]
async fn fetch_events_caps_range_to_max_blocks_per_fetch() {
    const MAX_BLOCKS_PER_FETCH: u64 = 100;
    const START_BLOCK_NUMBER: u64 = 0;
    const LATEST_BLOCK_NUMBER: u64 = 1000;
    const L1_BLOCK_HASH: L1BlockHash = L1BlockHash([7; 32]);
    // Inclusive window [start+1 ..= start+max], so the expected end is start + max.
    const EXPECTED_WINDOW_END: u64 = START_BLOCK_NUMBER + MAX_BLOCKS_PER_FETCH;

    let mut base_layer = MockBaseLayerContract::new();
    base_layer.expect_latest_l1_block_number().returning(|| Ok(LATEST_BLOCK_NUMBER));
    base_layer
        .expect_l1_block_at()
        .returning(move |number| Ok(Some(L1BlockReference { number, hash: L1_BLOCK_HASH })));
    // The requested range width must be exactly the configured cap.
    base_layer
        .expect_events()
        .withf(|block_range, _| {
            *block_range.start() == START_BLOCK_NUMBER + 1
                && *block_range.end() == EXPECTED_WINDOW_END
        })
        .times(1)
        .returning(|_, _| Ok(vec![]));

    let mut scraper = scraper_with_dummy().await;
    scraper.config.max_blocks_per_fetch = MAX_BLOCKS_PER_FETCH;
    scraper.scrape_from_this_l1_block =
        Some(L1BlockReference { number: START_BLOCK_NUMBER, hash: L1_BLOCK_HASH });
    scraper.base_layer = base_layer;

    let (window_end_block, events) = scraper.fetch_events().await.unwrap();
    assert!(events.is_empty());
    assert_eq!(window_end_block.number, EXPECTED_WINDOW_END);
}

// When the backlog is smaller than one window, the fetch requests only up to the latest
// (finality-adjusted) block and advances the cursor to it.
#[tokio::test]
async fn fetch_events_partial_window_stops_at_latest() {
    const MAX_BLOCKS_PER_FETCH: u64 = 1000;
    const START_BLOCK_NUMBER: u64 = 10;
    const LATEST_BLOCK_NUMBER: u64 = 25;
    const L1_BLOCK_HASH: L1BlockHash = L1BlockHash([7; 32]);

    let mut base_layer = MockBaseLayerContract::new();
    base_layer.expect_latest_l1_block_number().returning(|| Ok(LATEST_BLOCK_NUMBER));
    base_layer
        .expect_l1_block_at()
        .returning(move |number| Ok(Some(L1BlockReference { number, hash: L1_BLOCK_HASH })));
    base_layer
        .expect_events()
        .withf(|block_range, _| {
            *block_range.start() == START_BLOCK_NUMBER + 1
                && *block_range.end() == LATEST_BLOCK_NUMBER
        })
        .times(1)
        .returning(|_, _| Ok(vec![]));

    let mut scraper = scraper_with_dummy().await;
    scraper.config.max_blocks_per_fetch = MAX_BLOCKS_PER_FETCH;
    scraper.scrape_from_this_l1_block =
        Some(L1BlockReference { number: START_BLOCK_NUMBER, hash: L1_BLOCK_HASH });
    scraper.base_layer = base_layer;

    let (window_end_block, _events) = scraper.fetch_events().await.unwrap();
    assert_eq!(window_end_block.number, LATEST_BLOCK_NUMBER);
}

// The window ceiling is finality-adjusted: it must never request beyond latest - finality.
#[tokio::test]
async fn fetch_events_window_respects_finality_ceiling() {
    const MAX_BLOCKS_PER_FETCH: u64 = 1000;
    const FINALITY: u64 = 6;
    const START_BLOCK_NUMBER: u64 = 50;
    const LATEST_BLOCK_NUMBER: u64 = 60;
    const L1_BLOCK_HASH: L1BlockHash = L1BlockHash([7; 32]);
    // The backlog (10 blocks) is smaller than the window, so the ceiling is latest - finality.
    const EXPECTED_WINDOW_END: u64 = LATEST_BLOCK_NUMBER - FINALITY;

    let mut base_layer = MockBaseLayerContract::new();
    base_layer.expect_latest_l1_block_number().returning(|| Ok(LATEST_BLOCK_NUMBER));
    base_layer
        .expect_l1_block_at()
        .returning(move |number| Ok(Some(L1BlockReference { number, hash: L1_BLOCK_HASH })));
    base_layer
        .expect_events()
        .withf(|block_range, _| *block_range.end() == EXPECTED_WINDOW_END)
        .times(1)
        .returning(|_, _| Ok(vec![]));

    let mut scraper = scraper_with_dummy().await;
    scraper.config.max_blocks_per_fetch = MAX_BLOCKS_PER_FETCH;
    scraper.config.finality = FINALITY;
    scraper.scrape_from_this_l1_block =
        Some(L1BlockReference { number: START_BLOCK_NUMBER, hash: L1_BLOCK_HASH });
    scraper.base_layer = base_layer;

    let (window_end_block, _events) = scraper.fetch_events().await.unwrap();
    assert_eq!(window_end_block.number, EXPECTED_WINDOW_END);
}

// A large backlog is drained one window per poll until the cursor reaches latest.
#[tokio::test]
async fn catch_up_drains_backlog_over_multiple_polls() {
    const MAX_BLOCKS_PER_FETCH: u64 = 100;
    const LATEST_BLOCK_NUMBER: u64 = 250;
    const L1_BLOCK_HASH: L1BlockHash = L1BlockHash([7; 32]);

    let mut base_layer = MockBaseLayerContract::new();
    base_layer.expect_latest_l1_block_number().returning(|| Ok(LATEST_BLOCK_NUMBER));
    base_layer
        .expect_l1_block_at()
        .returning(move |number| Ok(Some(L1BlockReference { number, hash: L1_BLOCK_HASH })));
    base_layer.expect_events().returning(|_, _| Ok(vec![]));

    let mut l1_events_provider_client = MockL1EventsProviderClient::default();
    l1_events_provider_client.expect_add_events().returning(|_| Ok(()));

    let mut scraper = scraper_with_dummy().await;
    scraper.config.max_blocks_per_fetch = MAX_BLOCKS_PER_FETCH;
    scraper.scrape_from_this_l1_block = Some(L1BlockReference { number: 0, hash: L1_BLOCK_HASH });
    scraper.base_layer = base_layer;
    scraper.l1_events_provider_client = Arc::new(l1_events_provider_client);

    // Each poll advances by one window (100, 200, then 250 = latest).
    let expected_cursor_progression = [100, 200, LATEST_BLOCK_NUMBER];
    for expected_cursor in expected_cursor_progression {
        scraper.send_events_to_l1_events_provider().await.unwrap();
        assert_eq!(scraper.scrape_from_this_l1_block.unwrap().number, expected_cursor);
    }
}

// A convertible LogMessageToL2 event: calldata must lead with a from_address for msg-hash calc.
fn log_message_to_l2_event() -> L1Event {
    L1Event::LogMessageToL2 {
        tx: L1HandlerTransaction {
            calldata: Calldata(vec![Felt::ONE].into()),
            ..Default::default()
        },
        fee: Fee::default(),
        l1_tx_hash: None,
        block_timestamp: BlockTimestamp::default(),
    }
}

// A getLogs failure must not advance the cursor; the same window is retried on the next poll.
#[tokio::test]
async fn cursor_not_advanced_on_events_rpc_failure() {
    const START_BLOCK_NUMBER: u64 = 42;
    const MAX_BLOCKS_PER_FETCH: u64 = 1000;
    const LATEST_BLOCK_NUMBER: u64 = START_BLOCK_NUMBER + 5;
    const L1_BLOCK_HASH: L1BlockHash = L1BlockHash([7; 32]);

    let mut base_layer = MockBaseLayerContract::new();
    base_layer.expect_latest_l1_block_number().returning(|| Ok(LATEST_BLOCK_NUMBER));
    base_layer
        .expect_l1_block_at()
        .returning(move |number| Ok(Some(L1BlockReference { number, hash: L1_BLOCK_HASH })));
    base_layer.expect_events().returning(|_, _| Err(MockError::MockError));

    let mut scraper = scraper_with_dummy().await;
    scraper.config.max_blocks_per_fetch = MAX_BLOCKS_PER_FETCH;
    scraper.scrape_from_this_l1_block =
        Some(L1BlockReference { number: START_BLOCK_NUMBER, hash: L1_BLOCK_HASH });
    scraper.base_layer = base_layer;

    assert_matches!(
        scraper.send_events_to_l1_events_provider().await,
        Err(L1EventsScraperError::BaseLayerError(_))
    );
    assert_eq!(scraper.scrape_from_this_l1_block.unwrap().number, START_BLOCK_NUMBER);
}

// A commit (add_events) failure must not advance the cursor, so the events are not skipped.
#[tokio::test]
async fn cursor_not_advanced_on_add_events_failure() {
    const START_BLOCK_NUMBER: u64 = 42;
    const MAX_BLOCKS_PER_FETCH: u64 = 1000;
    const LATEST_BLOCK_NUMBER: u64 = START_BLOCK_NUMBER + 5;
    const L1_BLOCK_HASH: L1BlockHash = L1BlockHash([7; 32]);

    let mut base_layer = MockBaseLayerContract::new();
    base_layer.expect_latest_l1_block_number().returning(|| Ok(LATEST_BLOCK_NUMBER));
    base_layer
        .expect_l1_block_at()
        .returning(move |number| Ok(Some(L1BlockReference { number, hash: L1_BLOCK_HASH })));
    base_layer.expect_events().returning(|_, _| Ok(vec![log_message_to_l2_event()]));

    let mut l1_events_provider_client = MockL1EventsProviderClient::default();
    l1_events_provider_client.expect_add_events().once().returning(|_| {
        Err(apollo_l1_events_types::errors::L1EventsProviderClientError::L1EventsProviderError(
            L1EventsProviderError::Uninitialized,
        ))
    });

    let mut scraper = scraper_with_dummy().await;
    scraper.config.max_blocks_per_fetch = MAX_BLOCKS_PER_FETCH;
    scraper.scrape_from_this_l1_block =
        Some(L1BlockReference { number: START_BLOCK_NUMBER, hash: L1_BLOCK_HASH });
    scraper.base_layer = base_layer;
    scraper.l1_events_provider_client = Arc::new(l1_events_provider_client);

    assert!(scraper.send_events_to_l1_events_provider().await.is_err());
    assert_eq!(scraper.scrape_from_this_l1_block.unwrap().number, START_BLOCK_NUMBER);
}

// After a getLogs failure the retry must re-request the exact same [start, end] window (no bisect).
#[tokio::test]
async fn retry_refetches_same_window() {
    const START_BLOCK_NUMBER: u64 = 10;
    const MAX_BLOCKS_PER_FETCH: u64 = 100;
    const LATEST_BLOCK_NUMBER: u64 = 1000;
    const L1_BLOCK_HASH: L1BlockHash = L1BlockHash([7; 32]);
    // Inclusive window [start+1 ..= start+max], so the expected end is start + max.
    const EXPECTED_WINDOW_END: u64 = START_BLOCK_NUMBER + MAX_BLOCKS_PER_FETCH;

    let mut base_layer = MockBaseLayerContract::new();
    base_layer.expect_latest_l1_block_number().returning(|| Ok(LATEST_BLOCK_NUMBER));
    base_layer
        .expect_l1_block_at()
        .returning(move |number| Ok(Some(L1BlockReference { number, hash: L1_BLOCK_HASH })));
    // First attempt fails while requesting the capped window.
    base_layer
        .expect_events()
        .withf(|block_range, _| {
            *block_range.start() == START_BLOCK_NUMBER + 1
                && *block_range.end() == EXPECTED_WINDOW_END
        })
        .times(1)
        .returning(|_, _| Err(MockError::MockError));
    // The retry must request the identical window, not a bisected one.
    base_layer
        .expect_events()
        .withf(|block_range, _| {
            *block_range.start() == START_BLOCK_NUMBER + 1
                && *block_range.end() == EXPECTED_WINDOW_END
        })
        .times(1)
        .returning(|_, _| Ok(vec![]));

    let mut scraper = scraper_with_dummy().await;
    scraper.config.max_blocks_per_fetch = MAX_BLOCKS_PER_FETCH;
    scraper.scrape_from_this_l1_block =
        Some(L1BlockReference { number: START_BLOCK_NUMBER, hash: L1_BLOCK_HASH });
    scraper.base_layer = base_layer;

    assert_matches!(
        scraper.send_events_to_l1_events_provider().await,
        Err(L1EventsScraperError::BaseLayerError(_))
    );
    assert_eq!(scraper.scrape_from_this_l1_block.unwrap().number, START_BLOCK_NUMBER);

    scraper.send_events_to_l1_events_provider().await.unwrap();
    assert_eq!(scraper.scrape_from_this_l1_block.unwrap().number, EXPECTED_WINDOW_END);
}

// Capping the block range must never drop events: every event in the dense window is forwarded.
#[tokio::test]
async fn fetch_events_returns_all_events_in_dense_window() {
    const MAX_BLOCKS_PER_FETCH: u64 = 10;
    const START_BLOCK_NUMBER: u64 = 0;
    const LATEST_BLOCK_NUMBER: u64 = 1000;
    const NUM_EVENTS: usize = 500;
    const L1_BLOCK_HASH: L1BlockHash = L1BlockHash([7; 32]);
    // Inclusive window [start+1 ..= start+max], so the expected end is start + max.
    const EXPECTED_WINDOW_END: u64 = START_BLOCK_NUMBER + MAX_BLOCKS_PER_FETCH;

    let mut base_layer = MockBaseLayerContract::new();
    base_layer.expect_latest_l1_block_number().returning(|| Ok(LATEST_BLOCK_NUMBER));
    base_layer
        .expect_l1_block_at()
        .returning(move |number| Ok(Some(L1BlockReference { number, hash: L1_BLOCK_HASH })));
    base_layer
        .expect_events()
        .times(1)
        .returning(|_, _| Ok((0..NUM_EVENTS).map(|_| log_message_to_l2_event()).collect()));

    let forwarded_events: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(vec![]));
    let forwarded_events_clone = forwarded_events.clone();
    let mut l1_events_provider_client = MockL1EventsProviderClient::default();
    l1_events_provider_client.expect_add_events().once().returning(move |events| {
        *forwarded_events_clone.lock().unwrap() = events;
        Ok(())
    });

    let mut scraper = scraper_with_dummy().await;
    scraper.config.max_blocks_per_fetch = MAX_BLOCKS_PER_FETCH;
    scraper.scrape_from_this_l1_block =
        Some(L1BlockReference { number: START_BLOCK_NUMBER, hash: L1_BLOCK_HASH });
    scraper.base_layer = base_layer;
    scraper.l1_events_provider_client = Arc::new(l1_events_provider_client);

    scraper.send_events_to_l1_events_provider().await.unwrap();

    assert_eq!(forwarded_events.lock().unwrap().len(), NUM_EVENTS);
    assert_eq!(scraper.scrape_from_this_l1_block.unwrap().number, EXPECTED_WINDOW_END);
}

#[test]
#[ignore = "similar to backlog_happy_flow, only shorter, and sprinkle some start_block/get_txs \
            attempts while its catching up (and assert failure on height), then assert that they \
            succeed after catching up ends."]
fn catching_up_completion() {
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
