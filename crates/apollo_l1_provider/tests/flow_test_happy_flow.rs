#![cfg(any(test, feature = "testing"))]
mod utils;

use apollo_l1_provider_types::{
    InvalidValidationStatus,
    L1ProviderClient,
    SessionState,
    ValidationStatus,
};
use starknet_api::block::BlockNumber;
use utils::{
    send_message_from_l1_to_l2,
    setup_anvil_base_layer,
    setup_scraper_and_provider,
    CALL_DATA,
    CALL_DATA_2,
    COOLDOWN_DURATION,
    POLLING_INTERVAL_DURATION,
    ROUND_TO_SEC_MARGIN_DURATION,
    TARGET_L2_HEIGHT,
    WAIT_FOR_ASYNC_PROCESSING_DURATION,
};

#[tokio::test]
async fn new_l1_handler_tx_propose_validate_cooldown() {
    // Setup.

    // Setup the base layer.
    let mut base_layer = setup_anvil_base_layer().await;

    let (l2_hash, _nonce) = send_message_from_l1_to_l2(&mut base_layer, CALL_DATA).await;

    let l1_provider_client =
        setup_scraper_and_provider(base_layer.ethereum_base_layer.clone(), None).await;

    tokio::time::pause();

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
    tokio::time::advance(COOLDOWN_DURATION + ROUND_TO_SEC_MARGIN_DURATION).await;

    // Test that we propose after the cooldown is over.
    l1_provider_client.start_block(SessionState::Propose, next_block_height).await.unwrap();
    let txs = l1_provider_client.get_txs(n_txs, next_block_height).await.unwrap();
    assert!(!txs.is_empty());
    assert_eq!(txs[0].tx_hash, l2_hash);

    // Check that we can validate this message after the cooldown, too.
    l1_provider_client.start_block(SessionState::Validate, next_block_height).await.unwrap();
    assert_eq!(
        l1_provider_client.validate(l2_hash, next_block_height).await.unwrap(),
        ValidationStatus::Validated
    );

    // Commit this block.
    l1_provider_client.commit_block([l2_hash].into(), [].into(), next_block_height).await.unwrap();
    let new_height = next_block_height.unchecked_next();

    // Make sure the message is no longer available for validation/proposal.
    l1_provider_client.start_block(SessionState::Validate, new_height).await.unwrap();
    assert_eq!(
        l1_provider_client.validate(l2_hash, new_height).await.unwrap(),
        ValidationStatus::Invalid(InvalidValidationStatus::AlreadyIncludedOnL2)
    );
    l1_provider_client.start_block(SessionState::Propose, new_height).await.unwrap();
    let txs = l1_provider_client.get_txs(n_txs, new_height).await.unwrap();
    assert!(txs.is_empty());

    // Add another message to make sure we can scrape, validate, and propose it too.
    let (l2_hash_2, _nonce) = send_message_from_l1_to_l2(&mut base_layer, CALL_DATA_2).await;
    assert_ne!(l2_hash_2, l2_hash);

    // Wait for another scraping.
    tokio::time::advance(POLLING_INTERVAL_DURATION + ROUND_TO_SEC_MARGIN_DURATION).await;
    for _i in 0..100 {
        let snapshot = l1_provider_client.get_l1_provider_snapshot().await.unwrap();
        if snapshot.uncommitted_transactions.contains(&l2_hash_2) {
            break;
        }
        tokio::time::sleep(WAIT_FOR_ASYNC_PROCESSING_DURATION).await;
    }

    // Check that we can validate this message.
    l1_provider_client.start_block(SessionState::Validate, new_height).await.unwrap();
    assert_eq!(
        l1_provider_client.validate(l2_hash_2, new_height).await.unwrap(),
        ValidationStatus::Validated
    );

    // We can't propose it yet.
    l1_provider_client.start_block(SessionState::Propose, new_height).await.unwrap();
    let txs = l1_provider_client.get_txs(n_txs, new_height).await.unwrap();
    assert!(txs.is_empty());

    // Wait for the cooldown to be over.
    tokio::time::advance(COOLDOWN_DURATION + ROUND_TO_SEC_MARGIN_DURATION).await;

    // Check that we can propose it now.
    l1_provider_client.start_block(SessionState::Propose, new_height).await.unwrap();
    let txs = l1_provider_client.get_txs(n_txs, new_height).await.unwrap();
    assert!(!txs.is_empty());
}
