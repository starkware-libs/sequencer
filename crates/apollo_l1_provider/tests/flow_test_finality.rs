#![cfg(any(test, feature = "testing"))]
mod utils;

use apollo_l1_provider_types::{
    InvalidValidationStatus,
    L1ProviderClient,
    SessionState,
    ValidationStatus,
};
use apollo_l1_scraper_config::config::L1ScraperConfig;
use starknet_api::block::BlockNumber;
use utils::{
    send_message_from_l1_to_l2,
    setup_anvil_base_layer,
    setup_scraper_and_provider,
    CALL_DATA,
    CALL_DATA_2,
    CHAIN_ID,
    ONE_SEC,
    POLLING_INTERVAL_DURATION,
    TARGET_L2_HEIGHT,
    WAIT_FOR_ASYNC_PROCESSING_DURATION,
};

#[tokio::test]
async fn only_scrape_after_finality() {
    // Setup.
    const FINALITY: u64 = 3;

    // Setup the base layer.
    let mut base_layer = setup_anvil_base_layer().await;

    let (l2_hash, _nonce) = send_message_from_l1_to_l2(&mut base_layer, CALL_DATA).await;

    let l1_scraper_config = L1ScraperConfig {
        finality: FINALITY,
        polling_interval_seconds: POLLING_INTERVAL_DURATION,
        chain_id: CHAIN_ID,
        ..Default::default()
    };
    let l1_provider_client =
        setup_scraper_and_provider(base_layer.ethereum_base_layer.clone(), Some(l1_scraper_config))
            .await;

    tokio::time::pause();

    // Test.
    let next_block_height = BlockNumber(TARGET_L2_HEIGHT.0 + 1);

    // Check that we can validate this message even though no time has passed.
    l1_provider_client.start_block(SessionState::Validate, next_block_height).await.unwrap();
    assert_eq!(
        l1_provider_client.validate(l2_hash, next_block_height).await.unwrap(),
        ValidationStatus::Invalid(InvalidValidationStatus::NotFound)
    );

    // Send few more blocks (by sending more txs).
    for _ in 0..FINALITY {
        let (other_l2_hash, _nonce) =
            send_message_from_l1_to_l2(&mut base_layer, CALL_DATA_2).await;
        assert_ne!(l2_hash, other_l2_hash);
    }

    // Wait for another scraping.
    tokio::time::advance(POLLING_INTERVAL_DURATION + ONE_SEC).await;
    for _i in 0..100 {
        let snapshot = l1_provider_client.get_l1_provider_snapshot().await.unwrap();
        if snapshot.uncommitted_transactions.contains(&l2_hash) {
            break;
        }
        tokio::time::sleep(WAIT_FOR_ASYNC_PROCESSING_DURATION).await;
    }

    // Check that we can validate the message now.
    assert_eq!(
        l1_provider_client.validate(l2_hash, next_block_height).await.unwrap(),
        ValidationStatus::Validated
    );
}
