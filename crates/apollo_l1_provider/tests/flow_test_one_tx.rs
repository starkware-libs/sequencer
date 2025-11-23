mod utils;
use std::time::Duration;

use apollo_l1_provider_types::{L1ProviderClient, SessionState, ValidationStatus};
use starknet_api::block::BlockNumber;
use utils::{
    send_message_from_l1_to_l2,
    setup_anvil_base_layer,
    setup_scraper_and_provider,
    CALL_DATA,
    COOLDOWN_DURATION,
    TARGET_L2_HEIGHT,
};

// Start the test paused
#[tokio::test(start_paused = true)]
async fn new_l1_handler_tx_propose_validate_cooldown() {
    // Setup.

    // Setup the base layer.
    let base_layer = setup_anvil_base_layer().await;

    let (l2_hash, _nonce) = send_message_from_l1_to_l2(&base_layer, CALL_DATA).await;

    let l1_provider_client =
        setup_scraper_and_provider(base_layer.ethereum_base_layer.clone()).await;

    // Test.
    let next_block_height = BlockNumber(TARGET_L2_HEIGHT.0 + 1);

    // Check that we can validate this message even though no time has passed.
    l1_provider_client.start_block(SessionState::Validate, next_block_height).await.unwrap();
    assert_eq!(
        l1_provider_client.validate(l2_hash, next_block_height).await.unwrap(),
        ValidationStatus::Validated
    );

    // Test that we do not propose anything before the cooldown is over.
    l1_provider_client.start_block(SessionState::Propose, next_block_height).await.unwrap();
    let n_txs = 1;
    let txs = l1_provider_client.get_txs(n_txs, next_block_height).await.unwrap();
    assert!(txs.is_empty());

    // Sleep at least one second more than the cooldown to make sure we are not failing due to
    // fractional seconds.
    tokio::time::advance(COOLDOWN_DURATION + Duration::from_secs(1)).await;

    // Test that we propose after the cooldown is over.
    l1_provider_client.start_block(SessionState::Propose, next_block_height).await.unwrap();
    let txs = l1_provider_client.get_txs(n_txs, next_block_height).await.unwrap();
    assert!(!txs.is_empty());

    // Check that we can validate this message after the cooldown, too.
    l1_provider_client.start_block(SessionState::Validate, next_block_height).await.unwrap();
    assert_eq!(
        l1_provider_client.validate(l2_hash, next_block_height).await.unwrap(),
        ValidationStatus::Validated
    );
}
