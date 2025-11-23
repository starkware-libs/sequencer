use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use apollo_l1_provider_types::errors::L1ProviderError;
use apollo_l1_provider_types::MockL1ProviderClient;
use apollo_l1_scraper_config::config::L1ScraperConfig;
use assert_matches::assert_matches;
use papyrus_base_layer::{L1BlockHash, L1BlockReference, MockBaseLayerContract};

use crate::event_identifiers_to_track;
use crate::l1_scraper::{L1Scraper, L1ScraperError};

fn dummy_base_layer() -> MockBaseLayerContract {
    let mut base_layer = MockBaseLayerContract::new();
    base_layer.expect_latest_l1_block_number().return_once(|| Ok(Default::default()));
    base_layer.expect_events().return_once(|_, _| Ok(Default::default()));
    base_layer
}

async fn scraper_with_dummy() -> L1Scraper<MockBaseLayerContract> {
    let base_layer = dummy_base_layer();
    let mut l1_provider_client = MockL1ProviderClient::default();
    l1_provider_client.expect_add_events().returning(|_| Ok(()));

    let mut scraper = L1Scraper::new(
        L1ScraperConfig::default(),
        Arc::new(l1_provider_client),
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
    let mut l1_provider_client = MockL1ProviderClient::default();
    l1_provider_client.expect_add_events().once().returning(|_| {
        Err(apollo_l1_provider_types::errors::L1ProviderClientError::L1ProviderError(
            L1ProviderError::Uninitialized,
        ))
    });
    let mut scraper = scraper_with_dummy().await;
    scraper.l1_provider_client = Arc::new(l1_provider_client);
    scraper.base_layer.expect_l1_block_at().returning(|_| Ok(Some(Default::default())));

    // Test.
    assert_eq!(scraper.send_events_to_l1_provider().await, Err(L1ScraperError::NeedsRestart));
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
    assert_eq!(scraper.send_events_to_l1_provider().await, Ok(()));

    // Simulate an L1 revert: the last processed l1 block no longer exists.
    *l1_block_at_response.lock().unwrap() = None;

    assert_matches!(
        scraper.send_events_to_l1_provider().await,
        Err(L1ScraperError::L1ReorgDetected { .. })
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
    assert_eq!(scraper.send_events_to_l1_provider().await, Ok(()));

    // Simulate a base layer returning a lower block number.
    latest_l1_block_number_response.store(L1_BAD_LATEST_NUMBER, Ordering::Relaxed);

    // Make sure we don't hit the reorg error in this scenario.
    scraper.assert_no_l1_reorgs().await.unwrap();

    // Should ignore and try again on the next interval, returning the same block reference.
    assert_eq!(scraper.fetch_events().await, Ok((expected_block_reference, vec![])));
}

#[tokio::test]
async fn base_layer_returns_block_number_below_finality() {
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
    scraper.send_events_to_l1_provider().await.unwrap();

    // Simulate a base layer returning a lower block number.
    latest_l1_block_number_response.store(WRONG_L1_BLOCK_NUMBER, Ordering::Relaxed);

    // The scraper should return a finality too high error.
    assert_matches!(
        scraper.send_events_to_l1_provider().await,
        Err(L1ScraperError::FinalityTooHigh { .. })
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
