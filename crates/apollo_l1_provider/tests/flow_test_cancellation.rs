mod common;
use std::time::Duration;

use apollo_l1_provider_types::{
    InvalidValidationStatus,
    L1ProviderClient,
    SessionState,
    ValidationStatus,
};
use common::{
    send_cancellation_request,
    send_message_from_l1_to_l2,
    setup_anvil_base_layer,
    setup_scraper_and_provider,
    COOLDOWN_MILLIS,
    SMALL_DELAY_MILLIS,
    TARGET_L2_HEIGHT,
};
use starknet_api::block::BlockNumber;

#[tokio::test]
async fn new_l1_handler_tx_propose_validate_cancellation_timelock() {
    // Setup.

    // Setup the base layer.
    let base_layer = setup_anvil_base_layer().await;

    let (l2_hash, nonce) = send_message_from_l1_to_l2(&base_layer).await;

    let l1_provider_client = setup_scraper_and_provider(&base_layer).await;

    // Test.
    let next_block_height = BlockNumber(TARGET_L2_HEIGHT.0 + 1);

    // Check that we can validate this message.
    l1_provider_client.start_block(SessionState::Validate, next_block_height).await.unwrap();
    assert_eq!(
        l1_provider_client.validate(l2_hash, next_block_height).await.unwrap(),
        ValidationStatus::Validated
    );

    // Wait until the cancellation timelock is over.
    tokio::time::sleep(Duration::from_millis(COOLDOWN_MILLIS * 2)).await;

    send_cancellation_request(&base_layer, nonce).await;
    // Leave enough time for the cancellation request to be scraped and sent to provider.
    tokio::time::sleep(Duration::from_millis(SMALL_DELAY_MILLIS * 2)).await;

    // Should still be able to validate.
    l1_provider_client.start_block(SessionState::Validate, next_block_height).await.unwrap();
    assert_eq!(
        l1_provider_client.validate(l2_hash, next_block_height).await.unwrap(),
        ValidationStatus::Validated
    );

    // Should not be able to propose.
    let n_txs = 1;
    l1_provider_client.start_block(SessionState::Propose, next_block_height).await.unwrap();
    let txs = l1_provider_client.get_txs(n_txs, next_block_height).await.unwrap();
    assert!(txs.is_empty());

    // Sleep at least two times the cooldown to make sure we are not failing due to fractional
    // seconds.
    tokio::time::sleep(Duration::from_millis(COOLDOWN_MILLIS * 2)).await;

    // Should no longer be able to validate.
    l1_provider_client.start_block(SessionState::Validate, next_block_height).await.unwrap();
    assert_eq!(
        l1_provider_client.validate(l2_hash, next_block_height).await.unwrap(),
        ValidationStatus::Invalid(InvalidValidationStatus::CancelledOnL2)
    );
}
