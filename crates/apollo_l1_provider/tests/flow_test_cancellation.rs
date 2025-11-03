mod utils;
use std::sync::Arc;
use std::time::Duration;

use apollo_l1_provider_types::{
    InvalidValidationStatus,
    L1ProviderClient,
    SessionState,
    ValidationStatus,
};
use starknet_api::block::BlockNumber;
use utils::{
    send_cancellation_finalization,
    send_cancellation_request,
    send_message_from_l1_to_l2,
    setup_anvil_base_layer,
    setup_scraper_and_provider,
    CALL_DATA,
    CALL_DATA_2,
    POLLING_INTERVAL_DURATION,
    TARGET_L2_HEIGHT,
    TIMELOCK_DURATION,
};

use crate::utils::WAIT_FOR_ASYNC_PROCESSING_DURATION;

#[tokio::test]
async fn new_l1_handler_tx_propose_validate_cancellation_timelock() {
    // Setup.
    // Setup the base layer.
    let base_layer = Arc::new(setup_anvil_base_layer().await);
    let base_layer_clone = base_layer.clone();

    let (l2_hash, nonce) = send_message_from_l1_to_l2(&base_layer, CALL_DATA).await;

    let l1_provider_client =
        setup_scraper_and_provider(base_layer.ethereum_base_layer.clone()).await;

    // Test.
    tokio::time::pause();
    let next_block_height = BlockNumber(TARGET_L2_HEIGHT.0 + 1);

    // Check that we can validate this message.
    l1_provider_client.start_block(SessionState::Validate, next_block_height).await.unwrap();
    assert_eq!(
        l1_provider_client.validate(l2_hash, next_block_height).await.unwrap(),
        ValidationStatus::Validated
    );

    send_cancellation_request(&base_layer_clone, CALL_DATA, nonce).await;

    // Wait for another scraping.
    tokio::time::advance(POLLING_INTERVAL_DURATION + Duration::from_secs(1)).await;

    // Keep trying to get the snapshot showing the cancellation
    for _i in 0..1000 {
        let snapshot = l1_provider_client.get_l1_provider_snapshot().await.unwrap();
        if snapshot.cancellation_started_on_l2.contains(&l2_hash) {
            break;
        }
        tokio::time::sleep(WAIT_FOR_ASYNC_PROCESSING_DURATION).await;
    }

    // Verify we have left the loop with the cancellation marked as started on L2.
    let snapshot = l1_provider_client.get_l1_provider_snapshot().await.unwrap();
    assert!(snapshot.cancellation_started_on_l2.contains(&l2_hash));
    assert_eq!(snapshot.number_of_txs_in_records, 1);

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

    // Sleep at least one second more than the timelock to make sure we are not failing due to
    // fractional seconds.
    tokio::time::advance(TIMELOCK_DURATION + Duration::from_secs(1)).await;

    // Should no longer be able to validate.
    l1_provider_client.start_block(SessionState::Validate, next_block_height).await.unwrap();
    assert_eq!(
        l1_provider_client.validate(l2_hash, next_block_height).await.unwrap(),
        ValidationStatus::Invalid(InvalidValidationStatus::CancelledOnL2)
    );

    // Still cannot propose.
    l1_provider_client.start_block(SessionState::Propose, next_block_height).await.unwrap();
    let txs = l1_provider_client.get_txs(n_txs, next_block_height).await.unwrap();
    assert!(txs.is_empty());

    // Cancellation on L2 is finished, we no longer propose or validate.
    // Must check the snapshot only AFTER we try to validate, since that triggers an update of the
    // record state.
    let snapshot = l1_provider_client.get_l1_provider_snapshot().await.unwrap();
    assert!(!snapshot.cancellation_started_on_l2.contains(&l2_hash));
    assert!(snapshot.cancelled_on_l2.contains(&l2_hash));
    assert_eq!(snapshot.number_of_txs_in_records, 1);

    send_cancellation_finalization(&base_layer, CALL_DATA, nonce).await;

    // Sleep at least one second more than the cooldown to make sure we are not failing due to
    // fractional seconds.
    tokio::time::advance(POLLING_INTERVAL_DURATION + Duration::from_secs(1)).await;

    // TODO(guyn): check that the event gets deleted, after we add that functionality.

    // Check that the scraper and provider are still working.
    let (new_l2_hash, _nonce) = send_message_from_l1_to_l2(&base_layer, CALL_DATA_2).await;

    assert_ne!(new_l2_hash, l2_hash);

    // Wait for another scraping.
    tokio::time::advance(POLLING_INTERVAL_DURATION + Duration::from_secs(1)).await;

    for _i in 0..100 {
        let snapshot = l1_provider_client.get_l1_provider_snapshot().await.unwrap();
        if snapshot.uncommitted_transactions.contains(&new_l2_hash) {
            break;
        }
        tokio::time::sleep(WAIT_FOR_ASYNC_PROCESSING_DURATION).await;
    }
    // Check that we can validate this message.
    l1_provider_client.start_block(SessionState::Validate, next_block_height).await.unwrap();
    assert_eq!(
        l1_provider_client.validate(new_l2_hash, next_block_height).await.unwrap(),
        ValidationStatus::Validated
    );

    // The first tx is still cancelled.
    // TODO(guyn): after we implement cancellation deletion we should update this to "not found".
    assert_eq!(
        l1_provider_client.validate(l2_hash, next_block_height).await.unwrap(),
        ValidationStatus::Invalid(InvalidValidationStatus::CancelledOnL2)
    );
}
