#![cfg(any(test, feature = "testing"))]
mod utils;

use apollo_l1_events::l1_scraper::MAX_L1_HANDLER_PAYLOAD_LENGTH;
use apollo_l1_events_types::{
    InvalidValidationStatus,
    L1EventsProviderClient,
    SessionState,
    ValidationStatus,
};
use starknet_api::block::BlockNumber;
use utils::{
    send_message_from_l1_to_l2,
    setup_anvil_base_layer,
    setup_scraper_and_provider,
    TARGET_L2_HEIGHT,
};

fn payload_of_length(payload_length: usize) -> Vec<u8> {
    (1..=payload_length).map(|felt_value| u8::try_from(felt_value).unwrap()).collect()
}

// Proves the payload convention end to end through the real parse path: the L1 contract emits the
// payload, the base layer prepends from_address, and the scraper counts only the payload.
#[tokio::test]
async fn oversized_message_is_dropped_and_at_limit_message_is_scraped() {
    // Setup.
    let mut base_layer = setup_anvil_base_layer().await;
    let (oversized_hash, _nonce) = send_message_from_l1_to_l2(
        &mut base_layer,
        &payload_of_length(MAX_L1_HANDLER_PAYLOAD_LENGTH + 1),
    )
    .await;
    let (at_limit_hash, _nonce) = send_message_from_l1_to_l2(
        &mut base_layer,
        &payload_of_length(MAX_L1_HANDLER_PAYLOAD_LENGTH),
    )
    .await;

    // Both messages are already on L1, so the scraper's initialize path scrapes them.
    let l1_events_provider_client =
        setup_scraper_and_provider(base_layer.ethereum_base_layer.clone(), None).await;

    // Test.
    let snapshot = l1_events_provider_client.get_l1_events_provider_snapshot().await.unwrap();
    assert_eq!(snapshot.uncommitted_transactions, vec![at_limit_hash]);

    let next_block_height = BlockNumber(TARGET_L2_HEIGHT.0 + 1);
    l1_events_provider_client.start_block(SessionState::Validate, next_block_height).await.unwrap();
    assert_eq!(
        l1_events_provider_client.validate(at_limit_hash, next_block_height).await.unwrap(),
        ValidationStatus::Validated
    );
    assert_eq!(
        l1_events_provider_client.validate(oversized_hash, next_block_height).await.unwrap(),
        ValidationStatus::Invalid(InvalidValidationStatus::NotFound)
    );
}
