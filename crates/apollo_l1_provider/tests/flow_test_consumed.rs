mod utils;
use std::time::Duration;

use apollo_l1_provider_types::{L1ProviderClient, SessionState, ValidationStatus};
use papyrus_base_layer::{
    L1BlockHash,
    L1BlockNumber,
    L1BlockReference,
    L1Event,
    MockBaseLayerContract,
};
use starknet_api::block::{BlockNumber, BlockTimestamp};
use starknet_api::core::{EntryPointSelector, Nonce};
use starknet_api::transaction::fields::{Calldata, Fee};
use starknet_api::transaction::{L1HandlerTransaction, TransactionVersion};
use starknet_types_core::felt::Felt;
use utils::{setup_scraper_and_provider, COOLDOWN_DURATION, TARGET_L2_HEIGHT};

use crate::utils::{CALL_DATA, L1_CONTRACT_ADDRESS, L2_ENTRY_POINT};

fn block_from_number(number: L1BlockNumber) -> L1BlockReference {
    L1BlockReference { number, hash: L1BlockHash::default() }
}

#[tokio::test]
async fn l1_handler_tx_consumed_txs() {
    // Setup.
    // Make an a transaction to send from L1 to L2.
    let call_data: Vec<Felt> = CALL_DATA.iter().map(|x| Felt::from(*x)).collect();
    let l1_handler_tx = L1HandlerTransaction {
        version: TransactionVersion::default(),
        nonce: Nonce::default(),
        contract_address: L1_CONTRACT_ADDRESS.parse().unwrap(),
        entry_point_selector: EntryPointSelector(Felt::from_hex_unchecked(L2_ENTRY_POINT)),
        calldata: Calldata(call_data.into()),
    };

    // We will first send this message.
    let message_to_l2_event = L1Event::LogMessageToL2 {
        tx: l1_handler_tx.clone(),
        fee: Fee::default(),
        l1_tx_hash: None,
        block_timestamp: BlockTimestamp::default(),
    };
    // On the next time we scrape, we would find the consumed event.
    let message_consumed_event =
        L1Event::ConsumedMessageToL2 { tx: l1_handler_tx, timestamp: BlockTimestamp::default() };
    // This consumed event is sent only to trigger deletion on the provider records.
    let message_consumed_event_2 = L1Event::ConsumedMessageToL2 {
        tx: L1HandlerTransaction::default(),
        timestamp: BlockTimestamp::default(),
    };

    // Setup the base layer. Using a mock because we cannot actively cause a tx to be consumed
    // without a state update.
    let mut base_layer = MockBaseLayerContract::new();
    // The latest_l1_block and l1_block_at are used internally by the scraper.
    base_layer.expect_latest_l1_block().times(1).returning(move |_| Ok(Some(block_from_number(1))));
    base_layer.expect_latest_l1_block().times(1).returning(move |_| Ok(Some(block_from_number(2))));
    base_layer.expect_latest_l1_block().returning(move |_| Ok(Some(block_from_number(3))));
    base_layer
        .expect_l1_block_at()
        .returning(move |block_number| Ok(Some(block_from_number(block_number))));

    // First we get the message sent.
    base_layer
        .expect_events()
        .times(1)
        .returning(move |_range, _identifiers| Ok(vec![message_to_l2_event.clone()]));
    // Then we get the consumed event (should be scraped on the second iteration of the scraper
    // loop).
    base_layer
        .expect_events()
        .times(1)
        .returning(move |_range, _identifiers| Ok(vec![message_consumed_event.clone()]));
    // Finally, we get the second consumed event (to trigger deletion on the provider records).
    base_layer
        .expect_events()
        .times(1)
        .returning(move |_range, _identifiers| Ok(vec![message_consumed_event_2.clone()]));

    let l1_provider_client = setup_scraper_and_provider(base_layer).await;
    let snapshot = l1_provider_client.get_l1_provider_snapshot().await.unwrap();
    assert!(!snapshot.uncommitted_transactions.is_empty());
    assert_eq!(snapshot.number_of_txs_in_records, 1);
    let l2_hash = snapshot.uncommitted_transactions[0];

    // Test.
    let next_block_height = BlockNumber(TARGET_L2_HEIGHT.0 + 1);

    // Check that we can validate this message.
    l1_provider_client.start_block(SessionState::Validate, next_block_height).await.unwrap();
    assert_eq!(
        l1_provider_client.validate(l2_hash, next_block_height).await.unwrap(),
        ValidationStatus::Validated
    );

    // Sleep at least one second more than the cooldown to make sure we are not failing due to
    // fractional seconds.
    // After the polling interval has passed (=COOLDOWN_DURATION), the transaction should be
    // consumed.
    tokio::time::sleep(COOLDOWN_DURATION + Duration::from_secs(1)).await;

    let snapshot = l1_provider_client.get_l1_provider_snapshot().await.unwrap();
    assert!(!snapshot.consumed_transactions.is_empty());
    assert_eq!(snapshot.number_of_txs_in_records, 1);

    // Wait again to make sure the consumption timelock has passed.
    tokio::time::sleep(COOLDOWN_DURATION * 2 + Duration::from_secs(1)).await;

    let snapshot = l1_provider_client.get_l1_provider_snapshot().await.unwrap();
    assert!(snapshot.consumed_transactions.is_empty());
    assert_eq!(snapshot.number_of_txs_in_records, 0);
}
