use std::collections::HashSet;
use std::sync::Arc;

use apollo_batcher_types::communication::MockBatcherClient;
use apollo_l1_provider::l1_provider::L1ProviderBuilder;
use apollo_l1_provider::L1ProviderConfig;
use apollo_l1_provider_types::{
    Event,
    InvalidValidationStatus,
    MockL1ProviderClient,
    SessionState,
    ValidationStatus,
};
use apollo_state_sync_types::communication::MockStateSyncClient;
use starknet_api::block::BlockNumber;
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::hash::StarkHash;
use starknet_api::test_utils::l1_handler::{executable_l1_handler_tx, L1HandlerTxArgs};
use starknet_api::transaction::TransactionHash;

pub fn l1_handler(tx_hash: usize) -> L1HandlerTransaction {
    let tx_hash = TransactionHash(StarkHash::from(tx_hash));
    executable_l1_handler_tx(L1HandlerTxArgs { tx_hash, ..Default::default() })
}

#[tokio::test]
async fn cancellation_flow() {
    // Timelock of 2 blocks.
    const CANCELLATION_TIMELOCK_IN_BLOCKS: BlockNumber = BlockNumber(2);
    const START_HEIGHT: u64 = 1;

    let tx_1 = l1_handler(1);
    let tx_2 = l1_handler(2);
    let tx_3 = l1_handler(3);

    let mut provider = L1ProviderBuilder::new(
        L1ProviderConfig {
            cancellation_timelock_in_blocks: CANCELLATION_TIMELOCK_IN_BLOCKS,
            ..Default::default()
        },
        Arc::new(MockL1ProviderClient::default()),
        Arc::new(MockBatcherClient::default()),
        Arc::new(MockStateSyncClient::default()),
    )
    .startup_height(BlockNumber(START_HEIGHT))
    .catchup_height(BlockNumber(START_HEIGHT)) // Skip bootstrap, not relevant for this test.
    .build();

    provider
        .initialize(vec![
            Event::L1HandlerTransaction(tx_1.clone()),
            Event::L1HandlerTransaction(tx_2.clone()),
            Event::L1HandlerTransaction(tx_3.clone()),
        ])
        .await
        .unwrap();

    // Schedule cancellation of the second transaction.
    provider.add_events(vec![Event::TransactionCancellationStarted(tx_2.tx_hash)]).unwrap();

    // No timelock is due yet, `get_txs` should yield all three.
    provider.start_block(provider.current_height, SessionState::Propose).unwrap();
    let got_txs = provider.get_txs(10, provider.current_height).unwrap();
    assert_eq!(got_txs, vec![tx_1.clone(), tx_2.clone(), tx_3.clone()]);

    // Commit only the second one.
    provider.commit_block(&[tx_2.tx_hash], &HashSet::new(), provider.current_height).unwrap();
    // Schedule cancellation of the third transaction now.
    provider.add_events(vec![Event::TransactionCancellationStarted(tx_3.tx_hash)]).unwrap();

    // Timelock for second transaction is due, but it has already been committed, so NOP.
    provider.start_block(provider.current_height, SessionState::Validate).unwrap();
    // Verify status of all transactions:
    assert_eq!(
        provider.validate(tx_1.tx_hash, provider.current_height).unwrap(),
        ValidationStatus::Validated
    );
    assert_eq!(
        provider.validate(tx_2.tx_hash, provider.current_height).unwrap(),
        ValidationStatus::Invalid(InvalidValidationStatus::AlreadyIncludedOnL2)
    );
    assert_eq!(
        provider.validate(tx_3.tx_hash, provider.current_height).unwrap(),
        ValidationStatus::Validated
    );

    // Timelock for the third transaction not due yet.
    provider.commit_block(&[], &HashSet::new(), provider.current_height).unwrap();
    // Timelock for the third transaction is due but not applied yet, only in start_block (but can't
    // test this in a flow test, it'll show up in the upcoming start_block now).
    provider.commit_block(&[], &HashSet::new(), provider.current_height).unwrap();

    // The third transaction is cancelled here.
    provider.start_block(provider.current_height, SessionState::Validate).unwrap();
    assert_eq!(
        provider.validate(tx_1.tx_hash, provider.current_height).unwrap(),
        ValidationStatus::Validated
    );
    // Unknown because it was deleted.
    assert_eq!(
        provider.validate(tx_3.tx_hash, provider.current_height).unwrap(),
        ValidationStatus::Invalid(InvalidValidationStatus::ConsumedOnL1OrUnknown)
    );

    // Other node committed our last transaction.
    provider.commit_block(&[tx_1.tx_hash], &HashSet::new(), provider.current_height).unwrap();

    // Cancellation for the first transaction arrived after it was committed on L2, should be
    // dropped silently.
    provider.add_events(vec![Event::TransactionCancellationStarted(tx_2.tx_hash)]).unwrap();

    // Check that the first transaction is still in the committed buffer.
    provider.start_block(provider.current_height, SessionState::Validate).unwrap();
    assert_eq!(
        provider.validate(tx_1.tx_hash, provider.current_height).unwrap(),
        ValidationStatus::Invalid(InvalidValidationStatus::AlreadyIncludedOnL2)
    );
}
