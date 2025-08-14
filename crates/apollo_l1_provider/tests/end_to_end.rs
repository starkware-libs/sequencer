use std::sync::Arc;
use std::time::Duration;

use apollo_batcher_types::communication::MockBatcherClient;
use apollo_l1_provider::l1_provider::L1ProviderBuilder;
use apollo_l1_provider::L1ProviderConfig;
use apollo_l1_provider_types::InvalidValidationStatus::*;
use apollo_l1_provider_types::ValidationStatus::*;
use apollo_l1_provider_types::{Event, MockL1ProviderClient, SessionState};
use apollo_state_sync_types::communication::MockStateSyncClient;
use apollo_time::test_utils::FakeClock;
use apollo_time::time::Clock;
use pretty_assertions::assert_eq;
use starknet_api::block::{BlockNumber, BlockTimestamp};
use starknet_api::executable_transaction::L1HandlerTransaction as ExecutableL1HandlerTransaction;
use starknet_api::test_utils::l1_handler::{executable_l1_handler_tx, L1HandlerTxArgs};
use starknet_api::tx_hash;

use crate::SessionState::*;

fn cancellation_request(index: u64, cancellation_timestamp: BlockTimestamp) -> Event {
    Event::TransactionCancellationStarted {
        tx_hash: tx_hash!(index),
        cancellation_request_timestamp: cancellation_timestamp,
    }
}

/// For simplicity, we use a number for both the tx hash and the timestamp.
fn message_to_l2(index: u64) -> Event {
    Event::L1HandlerTransaction {
        l1_handler_tx: executable_l1_handler_tx(L1HandlerTxArgs {
            tx_hash: tx_hash!(index),
            ..Default::default()
        }),
        block_timestamp: index.into(),
        log_timestamp: index,
    }
}

fn l1_handler(index: u64) -> ExecutableL1HandlerTransaction {
    match message_to_l2(index) {
        Event::L1HandlerTransaction { l1_handler_tx, .. } => l1_handler_tx,
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn timing_flows() {
    let time_starts_at = 1;
    let clock = Arc::new(FakeClock::new(time_starts_at));

    let cancellation_timelock = 2;
    let new_message_cooldown = 1;
    let consumption_timelock = 1;
    let mut l1_provider = L1ProviderBuilder::new(
        L1ProviderConfig {
            l1_handler_cancellation_timelock_seconds: Duration::from_secs(cancellation_timelock),
            l1_handler_consumption_timelock_seconds: Duration::from_secs(consumption_timelock),
            new_l1_handler_cooldown_seconds: Duration::from_secs(new_message_cooldown),
            ..Default::default()
        },
        Arc::new(MockL1ProviderClient::default()),
        Arc::new(MockBatcherClient::default()),
        Arc::new(MockStateSyncClient::default()),
    )
    .startup_height(BlockNumber(1))
    .clock(clock.clone())
    .build();

    l1_provider
        .initialize(
            [
                cancellation_request(1, BlockTimestamp(1)), // Unknown, dropped silently.
                message_to_l2(2),
                message_to_l2(3),
                cancellation_request(2, BlockTimestamp(3)),
            ]
            .into(),
        )
        .await
        .unwrap();

    l1_provider
        .add_events(
            [message_to_l2(4), cancellation_request(3, BlockTimestamp(5)), message_to_l2(5)].into(),
        )
        .unwrap();

    // Ignored, an existing cancellation request on L2 is stronger than a new one.
    l1_provider.add_events([cancellation_request(3, BlockTimestamp(10))].into()).unwrap();

    l1_provider.commit_block([].into(), [].into(), l1_provider.current_height).unwrap();

    l1_provider.start_block(l1_provider.current_height, Propose).unwrap();
    // Everything's timelocked.
    assert_eq!(l1_provider.get_txs(2, l1_provider.current_height).unwrap(), []);

    clock.advance(Duration::from_secs(3));
    assert_eq!(clock.unix_now(), 4);

    // Cancellation request is stronger than new message cooldown, and prevents proposal forever.
    assert_eq!(l1_provider.get_txs(2, l1_provider.current_height).unwrap(), []);

    // But validate still works, cause cancellation timelock hasn't passed yet for anyone.
    l1_provider.start_block(l1_provider.current_height, Validate).unwrap();
    assert_eq!(l1_provider.validate(tx_hash!(2), l1_provider.current_height).unwrap(), Validated);
    assert_eq!(l1_provider.validate(tx_hash!(3), l1_provider.current_height).unwrap(), Validated);
    assert_eq!(l1_provider.validate(tx_hash!(4), l1_provider.current_height).unwrap(), Validated);

    clock.advance(Duration::from_secs(2));
    assert_eq!(clock.unix_now(), 6);
    // Passed timelock for the first non-cancelled transaction.
    l1_provider.start_block(l1_provider.current_height, Propose).unwrap();
    assert_eq!(l1_provider.get_txs(2, l1_provider.current_height).unwrap(), vec![l1_handler(4)]);

    // One of the l1 handlers is passed its cancellation timelock, no longer validatable.
    l1_provider.start_block(l1_provider.current_height, Validate).unwrap();
    for _ in 0..2 {
        assert_eq!(
            l1_provider.validate(tx_hash!(2), l1_provider.current_height).unwrap(),
            Invalid(CancelledOnL2)
        ); // Check twice for idempotency.
    }
    assert_eq!(l1_provider.validate(tx_hash!(3), l1_provider.current_height).unwrap(), Validated);

    clock.advance(Duration::from_secs(1));
    assert_eq!(clock.unix_now(), 7);
    // First cancellation request for this l1 handler counts, the second one that delayed the
    // cancellation was ignored.
    assert_eq!(
        l1_provider.validate(tx_hash!(3), l1_provider.current_height).unwrap(),
        Invalid(CancelledOnL2)
    );

    // Commit beats cancellations.
    l1_provider
        .commit_block(
            [tx_hash!(2), tx_hash!(3), tx_hash!(4)].into(),
            [].into(),
            l1_provider.current_height,
        )
        .unwrap();
    l1_provider.start_block(l1_provider.current_height, Validate).unwrap();
    for tx_hash in 2..=4 {
        assert_eq!(
            l1_provider.validate(tx_hash!(tx_hash), l1_provider.current_height).unwrap(),
            Invalid(AlreadyIncludedOnL2)
        );
    }

    // Cancel request on staged tx applied immediately, and persists for validate/propose after
    // new block started.
    assert_eq!(l1_provider.validate(tx_hash!(5), l1_provider.current_height).unwrap(), Validated);
    l1_provider.add_events([cancellation_request(5, BlockTimestamp(7))].into()).unwrap();
    clock.advance(Duration::from_secs(cancellation_timelock));
    assert_eq!(clock.unix_now(), 9);
    assert_eq!(
        l1_provider.validate(tx_hash!(5), l1_provider.current_height).unwrap(),
        Invalid(CancelledOnL2)
    );
    // Persists after new block start.
    l1_provider.start_block(l1_provider.current_height, Validate).unwrap();
    assert_eq!(
        l1_provider.validate(tx_hash!(5), l1_provider.current_height).unwrap(),
        Invalid(CancelledOnL2)
    );
    l1_provider.start_block(l1_provider.current_height, Propose).unwrap();
    assert_eq!(l1_provider.get_txs(2, l1_provider.current_height).unwrap(), []);

    // New cancellations are also ignored if cancellations have expired.
    l1_provider.add_events([cancellation_request(5, BlockTimestamp(10))].into()).unwrap();
    l1_provider.start_block(l1_provider.current_height, Validate).unwrap();
    assert_eq!(
        l1_provider.validate(tx_hash!(5), l1_provider.current_height).unwrap(),
        Invalid(CancelledOnL2)
    );
}
