use std::sync::Arc;
use std::time::Duration;

use apollo_batcher_types::communication::MockBatcherClient;
use apollo_l1_provider_types::errors::L1ProviderError;
use apollo_l1_provider_types::SessionState::{
    self,
    Propose as ProposeSession,
    Validate as ValidateSession,
};
use apollo_l1_provider_types::{Event, InvalidValidationStatus, ValidationStatus};
use apollo_state_sync_types::communication::MockStateSyncClient;
use apollo_time::test_utils::FakeClock;
use assert_matches::assert_matches;
use itertools::Itertools;
use pretty_assertions::assert_eq;
use rstest::rstest;
use starknet_api::block::{BlockNumber, BlockTimestamp};
use starknet_api::test_utils::l1_handler::{executable_l1_handler_tx, L1HandlerTxArgs};
use starknet_api::transaction::TransactionHash;
use starknet_api::tx_hash;

use crate::bootstrapper::{Bootstrapper, CommitBlockBacklog, SyncTaskHandle};
use crate::l1_provider::L1Provider;
use crate::test_utils::{
    l1_handler,
    ConsumedTransaction,
    FakeL1ProviderClient,
    L1ProviderContentBuilder,
};
use crate::{L1ProviderConfig, ProviderState};

fn commit_block_no_rejected(
    l1_provider: &mut L1Provider,
    txs: &[TransactionHash],
    block_number: BlockNumber,
) {
    l1_provider.commit_block(txs.iter().copied().collect(), [].into(), block_number).unwrap();
}

fn setup_rejected_transactions() -> L1Provider {
    let tx_id1 = 1;
    let tx_id2 = 2;
    let tx_id3 = 3;
    let first_block_number = BlockNumber(0);

    let tx1 = tx_hash!(tx_id1);
    let tx2 = tx_hash!(tx_id2);

    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_txs([l1_handler(tx_id1), l1_handler(tx_id2), l1_handler(tx_id3)])
        .with_height(first_block_number)
        .with_state(ProviderState::Propose)
        .build_into_l1_provider();

    // Commit block with rejected transactions.
    let consumed_txs = [tx1, tx2];
    let rejected_txs = [tx1];
    l1_provider.commit_block(consumed_txs.into(), rejected_txs.into(), first_block_number).unwrap();

    // Set the state to Validate for the validation tests.
    l1_provider.state = ProviderState::Validate;
    l1_provider
}

macro_rules! bootstrapper {
    (backlog: [$($height:literal => [$($tx:literal),* $(,)*]),* $(,)*], catch_up: $catch:expr) => {{
        Bootstrapper {
            commit_block_backlog: vec![
                $(CommitBlockBacklog {
                    height: BlockNumber($height),
                    committed_txs: [$(tx_hash!($tx)),*].into()
                }),*
            ].into_iter().collect(),
            catch_up_height: Arc::new(BlockNumber($catch).into()),
            l1_provider_client: Arc::new(FakeL1ProviderClient::default()),
            batcher_client: Arc::new(MockBatcherClient::default()),
            sync_client: Arc::new(MockStateSyncClient::default()),
            sync_task_handle: SyncTaskHandle::default(),
            n_sync_health_check_failures: Default::default(),
            sync_retry_interval: Duration::from_millis(10)
        }
    }};
}

/// Use to easily construct l1 handler messages for tests that don't care about the timestamp.
fn l1_handler_event(tx_hash: TransactionHash) -> Event {
    let default_timestamp = 0.into();
    Event::L1HandlerTransaction {
        l1_handler_tx: executable_l1_handler_tx(L1HandlerTxArgs { tx_hash, ..Default::default() }),
        block_timestamp: default_timestamp,
        scrape_timestamp: default_timestamp.0,
    }
}

fn timed_l1_handler_event(tx_hash: TransactionHash, timestamp: BlockTimestamp) -> Event {
    Event::L1HandlerTransaction {
        l1_handler_tx: executable_l1_handler_tx(L1HandlerTxArgs { tx_hash, ..Default::default() }),
        block_timestamp: timestamp,
        scrape_timestamp: timestamp.0,
    }
}

fn cancellation_event(
    tx_hash: TransactionHash,
    cancellation_request_timestamp: BlockTimestamp,
) -> Event {
    Event::TransactionCancellationStarted { tx_hash, cancellation_request_timestamp }
}

#[test]
fn get_txs_happy_flow() {
    // Setup.
    let txs = [l1_handler(0), l1_handler(1), l1_handler(2)];
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_txs(txs.clone())
        .with_state(ProviderState::Propose)
        .build_into_l1_provider();

    // Test.
    assert_eq!(l1_provider.get_txs(0, BlockNumber(0)).unwrap(), []);
    assert_eq!(l1_provider.get_txs(1, BlockNumber(0)).unwrap(), [txs[0].clone()]);
    assert_eq!(l1_provider.get_txs(3, BlockNumber(0)).unwrap(), txs[1..=2]);
    assert_eq!(l1_provider.get_txs(1, BlockNumber(0)).unwrap(), []);
}

#[test]
fn validate_happy_flow() {
    // Setup.
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_txs([l1_handler(1)])
        .with_committed([l1_handler(2)])
        .with_consumed_txs([ConsumedTransaction { tx: l1_handler(3), timestamp: 0.into() }])
        .with_state(ProviderState::Validate)
        .build_into_l1_provider();

    // Test.
    assert_eq!(
        l1_provider.validate(tx_hash!(1), BlockNumber(0)).unwrap(),
        ValidationStatus::Validated
    );
    assert_eq!(
        l1_provider.validate(tx_hash!(2), BlockNumber(0)).unwrap(),
        ValidationStatus::Invalid(InvalidValidationStatus::AlreadyIncludedOnL2)
    );
    // Transaction was consumed on L1.
    assert_eq!(
        l1_provider.validate(tx_hash!(3), BlockNumber(0)).unwrap(),
        ValidationStatus::Invalid(InvalidValidationStatus::ConsumedOnL1)
    );
    // Transaction wasn't deleted after the validation.
    assert_eq!(
        l1_provider.validate(tx_hash!(1), BlockNumber(0)).unwrap(),
        ValidationStatus::Invalid(InvalidValidationStatus::AlreadyIncludedInProposedBlock)
    );
}

#[test]
fn process_events_happy_flow() {
    // Setup.
    for state in [ProviderState::Propose, ProviderState::Validate, ProviderState::Pending] {
        let mut l1_provider = L1ProviderContentBuilder::new()
            .with_txs([l1_handler(1)])
            .with_committed_hashes([])
            .with_state(state.clone())
            .build_into_l1_provider();

        // Test.
        l1_provider
            .add_events(vec![l1_handler_event(tx_hash!(4)), l1_handler_event(tx_hash!(3))])
            .unwrap();
        l1_provider.add_events(vec![l1_handler_event(tx_hash!(6))]).unwrap();

        let expected_l1_provider = L1ProviderContentBuilder::new()
            .with_txs([
                l1_handler(1),
                l1_handler(4),
                l1_handler(3),
                l1_handler(6),
            ])
            .with_committed_hashes([])
            // State should be unchanged.
            .with_state(state)
            .build();

        expected_l1_provider.assert_eq(&l1_provider);
    }
}

#[test]
fn process_events_committed_txs() {
    // Setup.
    let timestamp = 1;
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_timed_txs([(l1_handler(1), timestamp)])
        .with_timed_committed([(l1_handler(2), timestamp)])
        .with_state(ProviderState::Pending)
        .build_into_l1_provider();

    let expected_l1_provider = l1_provider.clone();

    // Test.
    // Uncommitted transaction, should fail silently.
    l1_provider.add_events(vec![timed_l1_handler_event(tx_hash!(1), timestamp.into())]).unwrap();
    assert_eq!(l1_provider, expected_l1_provider);

    // Committed transaction, should fail silently.
    l1_provider.add_events(vec![timed_l1_handler_event(tx_hash!(2), timestamp.into())]).unwrap();
    assert_eq!(l1_provider, expected_l1_provider);
}

#[test]
fn pending_state_errors() {
    // Setup.
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_state(ProviderState::Pending)
        .with_txs([l1_handler(1)])
        .build_into_l1_provider();

    // Test.
    assert_matches!(
        l1_provider.get_txs(1, BlockNumber(0)).unwrap_err(),
        L1ProviderError::OutOfSessionGetTransactions
    );

    assert_matches!(
        l1_provider.validate(tx_hash!(1), BlockNumber(0)).unwrap_err(),
        L1ProviderError::OutOfSessionValidate
    );
}

#[test]
fn proposal_start_multiple_proposals_same_height() {
    // Setup.
    let mut l1_provider =
        L1ProviderContentBuilder::new().with_state(ProviderState::Pending).build_into_l1_provider();

    // Test all single-height combinations.
    const SESSION_TYPES: [SessionState; 2] = [ProposeSession, ValidateSession];
    for (session_1, session_2) in SESSION_TYPES.into_iter().cartesian_product(SESSION_TYPES) {
        l1_provider.start_block(BlockNumber(0), session_1).unwrap();
        l1_provider.start_block(BlockNumber(0), session_2).unwrap();
    }
}

#[test]
fn commit_block_empty_block() {
    // Setup.
    let txs = [l1_handler(1), l1_handler(2)];
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_txs(txs.clone())
        .with_height(BlockNumber(10))
        .with_state(ProviderState::Propose)
        .build_into_l1_provider();

    // Test: empty commit_block
    commit_block_no_rejected(&mut l1_provider, &[], BlockNumber(10));

    let expected_l1_provider = L1ProviderContentBuilder::new()
        .with_txs(txs)
        .with_height(BlockNumber(11))
        .with_state(ProviderState::Pending)
        .build();
    expected_l1_provider.assert_eq(&l1_provider);
}

#[test]
fn commit_block_during_proposal() {
    // Setup.
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_txs([l1_handler(1), l1_handler(2), l1_handler(3)])
        .with_height(BlockNumber(5))
        .with_state(ProviderState::Propose)
        .build_into_l1_provider();

    // Test: commit block during proposal.
    commit_block_no_rejected(&mut l1_provider, &[tx_hash!(1)], BlockNumber(5));

    let expected_l1_provider = L1ProviderContentBuilder::new()
        .with_txs([l1_handler(2), l1_handler(3)])
        .with_height(BlockNumber(6))
        .with_state(ProviderState::Pending)
        .build();
    expected_l1_provider.assert_eq(&l1_provider);
}

#[test]
fn commit_block_during_pending() {
    // Setup.
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_txs([l1_handler(1), l1_handler(2), l1_handler(3)])
        .with_height(BlockNumber(5))
        .with_state(ProviderState::Pending)
        .build_into_l1_provider();

    // Test: commit block during pending.
    commit_block_no_rejected(&mut l1_provider, &[tx_hash!(2)], BlockNumber(5));

    let expected_l1_provider = L1ProviderContentBuilder::new()
        .with_txs([l1_handler(1), l1_handler(3)])
        .with_height(BlockNumber(6))
        .with_state(ProviderState::Pending)
        .build();
    expected_l1_provider.assert_eq(&l1_provider);
}

#[test]
fn commit_block_during_validation() {
    // Setup.
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_txs([l1_handler(1), l1_handler(2), l1_handler(3)])
        .with_height(BlockNumber(5))
        .with_state(ProviderState::Validate)
        .build_into_l1_provider();

    // Test: commit block during validate.
    l1_provider.state = ProviderState::Validate;

    commit_block_no_rejected(&mut l1_provider, &[tx_hash!(3)], BlockNumber(5));
    let expected_l1_provider = L1ProviderContentBuilder::new()
        .with_txs([l1_handler(1), l1_handler(2)])
        .with_height(BlockNumber(6))
        .with_state(ProviderState::Pending)
        .build();
    expected_l1_provider.assert_eq(&l1_provider);
}

#[tokio::test]
async fn commit_block_backlog() {
    // Setup.
    let initial_bootstrap_state = ProviderState::Bootstrap(bootstrapper!(
        backlog: [10 => [2], 11 => [4]],
        catch_up: 9
    ));
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_txs([l1_handler(1), l1_handler(2), l1_handler(4)])
        .with_height(BlockNumber(8))
        .with_state(initial_bootstrap_state.clone())
        .build_into_l1_provider();

    l1_provider.initialize(vec![]).await.expect("l1 provider initialize failed");

    // Test.
    // Commit height too low to affect backlog.
    commit_block_no_rejected(&mut l1_provider, &[tx_hash!(1)], BlockNumber(8));
    let expected_l1_provider = L1ProviderContentBuilder::new()
        .with_txs([l1_handler(2), l1_handler(4)])
        .with_height(BlockNumber(9))
        .with_state(initial_bootstrap_state)
        .build();
    expected_l1_provider.assert_eq(&l1_provider);

    // Backlog is consumed, bootstrapping complete.
    commit_block_no_rejected(&mut l1_provider, &[], BlockNumber(9));

    let expected_l1_provider = L1ProviderContentBuilder::new()
        .with_txs([])
        .with_height(BlockNumber(12))
        .with_state(ProviderState::Pending)
        .build();
    expected_l1_provider.assert_eq(&l1_provider);
}

#[test]
fn commit_block_before_add_tx_stores_tx_in_committed() {
    // Setup
    let mut l1_provider =
        L1ProviderContentBuilder::new().with_committed([l1_handler(1)]).build_into_l1_provider();

    // Transactions unknown yet.
    commit_block_no_rejected(&mut l1_provider, &[tx_hash!(2), tx_hash!(3)], BlockNumber(0));
    let expected_l1_provider = L1ProviderContentBuilder::new()
        .with_txs([])
        .with_committed([l1_handler(1)])
        .with_committed_hashes([tx_hash!(2), tx_hash!(3)])
        .build();
    expected_l1_provider.assert_eq(&l1_provider);

    // Adding the tx after getting it from commit-block will store it as committed.
    l1_provider.add_events(vec![l1_handler_event(tx_hash!(2))]).unwrap();
    let expected_l1_provider = L1ProviderContentBuilder::new()
        .with_txs([])
        .with_committed([l1_handler(1), l1_handler(2)])
        .with_committed_hashes([tx_hash!(3)])
        .build();
    expected_l1_provider.assert_eq(&l1_provider);
}

#[tokio::test]
async fn bootstrap_commit_block_received_twice_no_error() {
    // Setup.
    let initial_bootstrap_state = ProviderState::Bootstrap(bootstrapper!(
        backlog: [],
        catch_up: 2
    ));
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_txs([l1_handler(1), l1_handler(2)])
        .with_state(initial_bootstrap_state)
        .build_into_l1_provider();

    l1_provider.initialize(vec![]).await.expect("l1 provider initialize failed");

    // Test.
    commit_block_no_rejected(&mut l1_provider, &[tx_hash!(1)], BlockNumber(0));
    // No error, since the this tx hash is already known to be committed.
    commit_block_no_rejected(&mut l1_provider, &[tx_hash!(1)], BlockNumber(0));
}

#[tokio::test]
async fn bootstrap_commit_block_received_twice_error_if_new_uncommitted_txs() {
    // Setup.
    let initial_bootstrap_state = ProviderState::Bootstrap(bootstrapper!(
        backlog: [],
        catch_up: 2
    ));
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_txs([l1_handler(1), l1_handler(2)])
        .with_state(initial_bootstrap_state)
        .build_into_l1_provider();

    l1_provider.initialize(vec![]).await.expect("l1 provider initialize failed");

    // Test.
    commit_block_no_rejected(&mut l1_provider, &[tx_hash!(1)], BlockNumber(0));
    // Error, since the new tx hash is not known to be committed.
    assert_matches!(
        l1_provider
            .commit_block([tx_hash!(1), tx_hash!(3)].into(), [].into(), BlockNumber(0))
            .unwrap_err(),
        L1ProviderError::UnexpectedHeight { expected_height: BlockNumber(1), got: BlockNumber(0) }
    );
}

#[tokio::test]
#[should_panic(expected = "Restart service")]
async fn restart_service_if_initialized_in_steady_state() {
    // Setup.
    let mut l1_provider =
        L1ProviderContentBuilder::new().with_state(ProviderState::Pending).build_into_l1_provider();

    // Test.
    l1_provider.initialize(vec![]).await.unwrap();
}

#[test]
fn commit_block_rejected_transactions() {
    let l1_provider = setup_rejected_transactions();

    // Ensure that the rejected and committed transaction is correctly tracked.
    let expected_l1_provider = L1ProviderContentBuilder::new()
        .with_txs([l1_handler(3)])
        .with_rejected([l1_handler(1)])
        .with_committed([l1_handler(2)])
        .with_height(BlockNumber(1))
        .build();
    expected_l1_provider.assert_eq(&l1_provider);
}

#[rstest]
#[case::validate_rejected_transaction(tx_hash!(1), ValidationStatus::Validated)]
#[case::validate_non_rejected_transaction(tx_hash!(2), ValidationStatus::Invalid(InvalidValidationStatus::AlreadyIncludedOnL2))]
#[case::validate_uncommitted_transaction(tx_hash!(3), ValidationStatus::Validated)]
fn validate_rejected_transactions(
    #[case] tx: TransactionHash,
    #[case] expected_status: ValidationStatus,
) {
    let mut l1_provider = setup_rejected_transactions();

    assert_eq!(l1_provider.validate(tx, BlockNumber(1)).unwrap(), expected_status);
}

#[test]
fn validate_same_rejected_transaction_twice() {
    let tx1 = tx_hash!(1);
    let mut l1_provider = setup_rejected_transactions();

    // Stage the rejected transaction.
    l1_provider.validate(tx1, BlockNumber(1)).unwrap();

    // Test: Validate already proposed rejected transaction.
    assert_eq!(
        l1_provider.validate(tx1, BlockNumber(1)).unwrap(),
        ValidationStatus::Invalid(InvalidValidationStatus::AlreadyIncludedInProposedBlock)
    );
}

#[test]
fn validate_rejected_transaction_accepted_after_rollback() {
    let tx1 = tx_hash!(1);
    let mut l1_provider = setup_rejected_transactions();

    // Stage the rejected transaction.
    l1_provider.validate(tx1, BlockNumber(1)).unwrap();

    // Rollback the rejected transaction.
    l1_provider.start_block(BlockNumber(1), ValidateSession).unwrap();

    // Test: Validate already proposed rejected transaction.
    assert_eq!(l1_provider.validate(tx1, BlockNumber(1)).unwrap(), ValidationStatus::Validated);
}

#[test]
fn add_new_transaction_not_added_if_rejected() {
    // Setup.
    let rejected_tx_id: TransactionHash = tx_hash!(1);
    let mut l1_provider = setup_rejected_transactions();

    // Add a new transaction that is already in the rejected set.
    l1_provider.add_events(vec![l1_handler_event(rejected_tx_id)]).unwrap();

    // Ensure that the rejected transaction is not re-added to the provider.
    let expected_l1_provider = L1ProviderContentBuilder::new()
        .with_txs([l1_handler(3)])
        .with_rejected([l1_handler(1)])
        .with_committed([l1_handler(2)])
        .with_height(BlockNumber(1))
        .build();
    expected_l1_provider.assert_eq(&l1_provider);

    // Ensure that the rejected transaction is not re-added to the provider, even if it is staged.
    l1_provider.validate(rejected_tx_id, BlockNumber(1)).unwrap();
    l1_provider.add_events(vec![l1_handler_event(rejected_tx_id)]).unwrap();
    expected_l1_provider.assert_eq(&l1_provider);
}

#[test]
#[should_panic(expected = "committed twice")]
fn commit_block_twice_panics() {
    // Setup.
    let mut l1_provider =
        L1ProviderContentBuilder::new().with_committed([l1_handler(1)]).build_into_l1_provider();

    // Test.
    l1_provider.commit_block([tx_hash!(1)].into(), [].into(), BlockNumber(0)).unwrap();
}

#[test]
fn add_tx_identical_timestamp_both_stored() {
    // Setup.
    let tx_1 = l1_handler(1);
    let tx_2 = l1_handler(2);
    let tx_3 = l1_handler(3);
    let timestamp_1 = 6;
    let timestamp_2 = timestamp_1;
    let timestamp_3 = 7;

    // Test.

    let mut l1_provider = L1ProviderContentBuilder::new().build_into_l1_provider();
    l1_provider
        .add_events(vec![
            timed_l1_handler_event(tx_1.clone().tx_hash, timestamp_1.into()),
            timed_l1_handler_event(tx_2.clone().tx_hash, timestamp_2.into()),
            timed_l1_handler_event(tx_3.clone().tx_hash, timestamp_3.into()),
        ])
        .unwrap();

    // Should contain txs even if they have identical timestamp.
    let expected = L1ProviderContentBuilder::new()
        .with_timed_txs([(tx_1, timestamp_1), (tx_2, timestamp_2), (tx_3, timestamp_3)])
        .build();
    expected.assert_eq(&l1_provider);
}

#[test]
fn get_txs_same_timestamp_returns_in_arrival_order() {
    // Setup.
    let tx1 = l1_handler(100);
    let tx2 = l1_handler(200);
    let tx3 = l1_handler(300);
    let timestamp_1_2 = 1;
    let timestamp_3 = 2;
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_timed_txs([
            (tx1.clone(), timestamp_1_2),
            (tx2.clone(), timestamp_1_2),
            (tx3.clone(), timestamp_3),
        ])
        .with_state(ProviderState::Propose)
        .build_into_l1_provider();

    // Test.
    let expected = [tx1.clone(), tx2.clone(), tx3.clone()];
    assert_eq!(
        l1_provider.get_txs(10, l1_provider.current_height).unwrap(),
        expected,
        "Transactions with the same timestamp must be returned in order of arrival"
    );

    // Now with a different order for the equal-timestamped ones.
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_timed_txs([
            (tx2.clone(), timestamp_1_2),
            (tx1.clone(), timestamp_1_2),
            (tx3.clone(), timestamp_3),
        ])
        .with_state(ProviderState::Propose)
        .build_into_l1_provider();

    // Test.
    let expected = vec![tx2, tx1, tx3];
    assert_eq!(
        l1_provider.get_txs(10, l1_provider.current_height).unwrap(),
        expected,
        "Transactions with the same timestamp must be returned in order of arrival"
    );
}

#[test]
fn get_txs_identical_timestamps() {
    let tx_1 = l1_handler(1);
    let tx_2 = l1_handler(2);
    let tx_3 = l1_handler(3);
    let timestamp_1 = 1;
    let timestamp_2 = timestamp_1; // Transaction 2 has the same timestamp as 1.
    let timestamp_3 = 2;

    let l1_provider_builder = L1ProviderContentBuilder::new()
        .with_timed_txs([
            (tx_1.clone(), timestamp_1),
            (tx_2.clone(), timestamp_2),
            (tx_3.clone(), timestamp_3),
        ])
        .with_state(ProviderState::Propose);

    // Can get only one tx out of the two with the same timestamp.
    assert_eq!(
        l1_provider_builder.clone().build_into_l1_provider().get_txs(1, BlockNumber(0)).unwrap(),
        [tx_1.clone()]
    );

    assert_eq!(
        l1_provider_builder.build_into_l1_provider().get_txs(3, BlockNumber(0)).unwrap(),
        [tx_1, tx_2, tx_3]
    );
}

#[test]
fn get_txs_timestamp_cutoff_some_eligible() {
    // Setup.
    let tx_1 = l1_handler(1);
    let tx_2 = l1_handler(2);
    let tx_3 = l1_handler(3);

    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_txs([tx_1.clone()])
        .with_timelocked_txs([tx_2, tx_3])
        .with_state(ProviderState::Propose)
        .build_into_l1_provider();

    // Test.
    let result = l1_provider.get_txs(10, BlockNumber(0)).unwrap();
    assert_eq!(result, vec![tx_1.clone()]);
}

#[test]
fn get_txs_timestamp_cutoff_none_eligible() {
    // Setup.
    let tx_1 = l1_handler(1);
    let tx_2 = l1_handler(2);
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_timelocked_txs([tx_1.clone(), tx_2.clone()])
        .with_state(ProviderState::Propose)
        .build_into_l1_provider();

    // Test.
    let result = l1_provider.get_txs(10, BlockNumber(0)).unwrap();
    assert_eq!(result, vec![]);
}

#[test]
fn get_txs_timestamp_cutoff_edge_case_at_cutoff() {
    let tx_1 = l1_handler(1);
    let tx_2 = l1_handler(2);
    let tx_3 = l1_handler(3);
    let timestamp_1 = 0;
    let timestamp_2 = 1;
    let timestamp_3 = 2;
    // Only timestamp 1 is passed cooldown, meaning, only it was created more than `cooldown`
    // seconds before `now`.
    let now = 2;
    let cooldown = 1;

    let clock = Arc::new(FakeClock::new(now));

    let config = L1ProviderConfig {
        new_l1_handler_cooldown_seconds: Duration::from_secs(cooldown),
        ..Default::default()
    };
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_config(config)
        .with_clock(clock)
        .with_timed_txs([
            (tx_1.clone(), timestamp_1),
            (tx_2.clone(), timestamp_2),
            (tx_3.clone(), timestamp_3),
        ])
        .with_state(ProviderState::Propose)
        .build_into_l1_provider();

    let result = l1_provider.get_txs(10, BlockNumber(0)).unwrap();
    assert_eq!(result, vec![tx_1.clone()]);
}

#[test]
fn get_txs_excludes_cancellation_requested_and_returns_non_cancellation_requested() {
    // Setup.
    let tx_1 = l1_handler(1);
    let tx_2 = l1_handler(2);
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_txs([tx_2.clone()])
        .with_cancel_requested_txs([tx_1.clone()])
        .with_state(ProviderState::Propose)
        .build_into_l1_provider();

    // Test.
    assert_eq!(l1_provider.get_txs(4, l1_provider.current_height).unwrap(), vec![tx_2.clone()]);
}

#[test]
fn get_txs_excludes_transaction_after_cancellation_expiry() {
    // Setup.
    let tx_1 = l1_handler(1);
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_cancelled_txs([tx_1.clone()])
        .with_state(ProviderState::Propose)
        .build_into_l1_provider();

    // Test.
    assert_eq!(l1_provider.get_txs(3, l1_provider.current_height).unwrap(), vec![]);
}

#[test]
fn validate_tx_cancellation_requested_not_expired_returns_validated() {
    // Setup.
    let tx_1 = l1_handler(1);
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_cancel_requested_txs([tx_1.clone()])
        .with_state(ProviderState::Validate)
        .build_into_l1_provider();

    // Test.
    let status = l1_provider.validate(tx_1.tx_hash, l1_provider.current_height).unwrap();
    assert_eq!(status, ValidationStatus::Validated);
}

#[test]
fn validate_tx_cancellation_requested_expired_returns_cancelled() {
    // Setup.
    let tx_1 = l1_handler(2);
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_cancelled_txs([tx_1.clone()])
        .with_state(ProviderState::Validate)
        .build_into_l1_provider();

    // Test.
    // Should return Invalid(CancelledOnL2),
    let status = l1_provider.validate(tx_1.tx_hash, l1_provider.current_height).unwrap();
    assert_eq!(status, InvalidValidationStatus::CancelledOnL2.into());
    // Idempotent.
    let status2 = l1_provider.validate(tx_1.tx_hash, l1_provider.current_height).unwrap();
    assert_eq!(status2, InvalidValidationStatus::CancelledOnL2.into());
}

#[test]
fn validate_tx_cancellation_requested_validated_then_expired_returns_cancelled() {
    // Setup.
    let tx_1 = l1_handler(1);
    let clock = Arc::new(FakeClock::new(5));
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_clock(clock.clone())
        .with_cancel_requested_txs([tx_1.clone()])
        .with_state(ProviderState::Validate)
        .build_into_l1_provider();

    // Test.

    // Should be validatable before expiry,
    let status = l1_provider.validate(tx_1.tx_hash, l1_provider.current_height).unwrap();
    assert_eq!(status, ValidationStatus::Validated);
    // Now, advance time past expiry and validate again,
    // This tests the edge case: a tx can be validatable before expiry, but if validated again after
    // expiry, it should return the cancellation error.
    clock.advance(Duration::from_secs(
        l1_provider.config.l1_handler_cancellation_timelock_seconds.as_secs(),
    ));
    let status2 = l1_provider.validate(tx_1.tx_hash, l1_provider.current_height).unwrap();
    assert_eq!(status2, InvalidValidationStatus::CancelledOnL2.into());
}

#[test]
fn commit_block_commits_cancellation_requested_tx_not_expired() {
    // Setup.
    let tx = l1_handler(1);
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_cancel_requested_txs([tx.clone()])
        .build_into_l1_provider();

    // Test.
    l1_provider.commit_block([tx.tx_hash].into(), [].into(), l1_provider.current_height).unwrap();
    let expected = L1ProviderContentBuilder::new().with_txs([]).with_committed([tx]).build();
    expected.assert_eq(&l1_provider);
}

#[test]
fn commit_block_commits_cancellation_requested_expired_and_fully_cancelled() {
    // Setup.
    let tx_1 = l1_handler(1);
    let tx_2 = l1_handler(2);
    let mut l1_provider = L1ProviderContentBuilder::new()
        // Both txs are passed cancellation request already, but still not in `Cancelled` state.
        .with_cancelled_txs([tx_1.clone(), tx_2.clone()])
        .with_state(ProviderState::Validate)
        .build_into_l1_provider();

    // Validate tx_2, which triggers the record to transition to state `CancelledOnL2`.
    l1_provider.validate(tx_2.tx_hash, l1_provider.current_height).unwrap();

    // Test.

    // Commit overrides both Cancelled state and CancellationStarted state.
    l1_provider
        .commit_block([tx_1.tx_hash, tx_2.tx_hash].into(), [].into(), l1_provider.current_height)
        .unwrap();

    let expected =
        L1ProviderContentBuilder::new().with_txs([]).with_committed([tx_1, tx_2]).build();
    expected.assert_eq(&l1_provider);
}

#[test]
fn commit_block_commits_mixed_normal_and_cancellation_requested() {
    // Setup.
    let tx_normal = l1_handler(1);
    let tx_cancel = l1_handler(2);
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_txs([tx_normal.clone()])
        .with_cancel_requested_txs([tx_cancel.clone()])
        .with_state(ProviderState::Propose)
        .build_into_l1_provider();

    // Test.
    let txs = [tx_normal.tx_hash, tx_cancel.tx_hash];
    l1_provider.commit_block(txs.into(), [].into(), l1_provider.current_height).unwrap();

    let expected =
        L1ProviderContentBuilder::new().with_txs([]).with_committed([tx_normal, tx_cancel]).build();
    expected.assert_eq(&l1_provider);
}

#[test]
fn add_events_tx_and_cancel_same_call_not_expired() {
    // Setup.
    let tx = l1_handler(1);
    let tx_hash = tx.tx_hash;
    let arbitrary_cancellation_timestamp = 1;
    let mut l1_provider = L1ProviderContentBuilder::new().build_into_l1_provider();

    // Test.
    let events = [
        l1_handler_event(tx_hash),
        cancellation_event(tx_hash, arbitrary_cancellation_timestamp.into()),
    ];
    l1_provider.add_events(events.into()).unwrap();
    let expected = L1ProviderContentBuilder::new()
        .with_timed_cancel_requested_txs([(tx.clone(), arbitrary_cancellation_timestamp)])
        .build();
    expected.assert_eq(&l1_provider);
}

#[test]
fn add_events_tx_then_cancel_separate_calls_not_expired() {
    // Setup.
    let tx = l1_handler(1);
    let tx_hash = tx.tx_hash;
    let arbitrary_cancellation_timestamp = 1;
    let mut l1_provider = L1ProviderContentBuilder::new().build_into_l1_provider();

    // Test.
    l1_provider.add_events(vec![l1_handler_event(tx_hash)]).unwrap();
    // Tests that cancellations are independent of when their tx was received
    l1_provider
        .add_events(vec![cancellation_event(tx_hash, arbitrary_cancellation_timestamp.into())])
        .unwrap();
    let expected = L1ProviderContentBuilder::new()
        .with_txs([])
        .with_timed_cancel_requested_txs([(tx.clone(), arbitrary_cancellation_timestamp)])
        .build();
    expected.assert_eq(&l1_provider);
}

#[test]
fn add_events_tx_and_cancel_same_call_expired() {
    // Setup.
    let tx = l1_handler(1);
    let now = 3;
    let timelock = 0; // all cancellations immediately expire.
    let cancellation_request_timestamp = now;
    let config = L1ProviderConfig {
        l1_handler_cancellation_timelock_seconds: Duration::from_secs(timelock),
        ..Default::default()
    };
    let clock = Arc::new(FakeClock::new(now));
    let events = [
        l1_handler_event(tx.tx_hash),
        cancellation_event(tx.tx_hash, cancellation_request_timestamp.into()),
    ];
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_config(config)
        .with_clock(clock)
        .with_state(ProviderState::Validate)
        .build_into_l1_provider();

    // Test.
    l1_provider.add_events(events.into()).unwrap();
    // Validate tx, which triggers the record to transition to state `CancelledOnL2`.
    l1_provider.validate(tx.tx_hash, l1_provider.current_height).unwrap();

    let expected = L1ProviderContentBuilder::new()
        .with_txs([])
        .with_timed_cancel_requested_txs([(tx.clone(), cancellation_request_timestamp)])
        .build();
    expected.assert_eq(&l1_provider);
}

#[test]
fn add_events_only_cancel_event_unknown_tx() {
    // Setup.
    let unknown_tx_hash = tx_hash!(2);
    let mut l1_provider = L1ProviderContentBuilder::new().build_into_l1_provider();

    // Test.
    l1_provider.add_events(vec![cancellation_event(unknown_tx_hash, 0.into())]).unwrap();
    let expected_empty =
        L1ProviderContentBuilder::new().with_txs([]).with_timed_cancel_requested_txs([]).build();
    expected_empty.assert_eq(&l1_provider);
}

#[test]
fn add_events_double_cancellation_only_first_counted() {
    // Setup.
    let tx = l1_handler(1);
    let tx_hash = tx.tx_hash;
    let cancellation_request_timestamp_first = 3;
    let cancellation_request_timestamp_second = 4;
    let mut l1_provider =
        L1ProviderContentBuilder::new().with_txs([tx.clone()]).build_into_l1_provider();

    // Test.

    l1_provider.add_events(vec![l1_handler_event(tx_hash)]).unwrap();
    l1_provider
        .add_events(vec![cancellation_event(tx_hash, cancellation_request_timestamp_first.into())])
        .unwrap();
    l1_provider
        .add_events(vec![cancellation_event(tx_hash, cancellation_request_timestamp_second.into())])
        .unwrap();
    // Only first cancellation counts.
    let expected = L1ProviderContentBuilder::new()
        .with_timed_cancel_requested_txs([(tx.clone(), cancellation_request_timestamp_first)])
        .build();
    expected.assert_eq(&l1_provider);
}

#[test]
fn validate_tx_unknown_returns_invalid_not_found() {
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_state(ProviderState::Validate)
        .build_into_l1_provider();
    // tx_1 was never added
    let status = l1_provider.validate(tx_hash!(1), l1_provider.current_height).unwrap();
    assert_eq!(status, InvalidValidationStatus::NotFound.into());
}

#[test]
fn commit_block_historical_height_short_circuits_non_bootstrap() {
    // Setup.
    let l1_provider_builder = L1ProviderContentBuilder::new()
        .with_height(BlockNumber(5))
        .with_txs([l1_handler(1)])
        .with_state(ProviderState::Propose);

    // Test.
    let mut l1_provider = l1_provider_builder.clone().build_into_l1_provider();
    let old_height = BlockNumber(4);
    l1_provider.commit_block([tx_hash!(1)].into(), [].into(), old_height).unwrap();

    let expected_unchanged = l1_provider_builder.build();
    expected_unchanged.assert_eq(&l1_provider);
}

#[tokio::test]
async fn commit_block_historical_height_short_circuits_bootstrap() {
    // Setup.
    let batcher_height_old = 4;
    let initial_bootstrap_state = ProviderState::Bootstrap(bootstrapper!(
        backlog: [],
        catch_up: batcher_height_old
    ));
    let l1_provider_builder = L1ProviderContentBuilder::new()
        .with_height(BlockNumber(5))
        .with_txs([l1_handler(1)])
        .with_state(initial_bootstrap_state);

    // Test.
    let mut l1_provider = l1_provider_builder.clone().build_into_l1_provider();

    l1_provider.initialize(vec![]).await.expect("l1 provider initialize failed");

    l1_provider
        .commit_block([tx_hash!(1)].into(), [].into(), BlockNumber(batcher_height_old))
        .unwrap();

    let expected_unchanged = l1_provider_builder.build();
    expected_unchanged.assert_eq(&l1_provider);
}

#[test]
fn consuming_committed_tx() {
    // Setup.
    let tx = l1_handler(1);
    let mut l1_provider =
        L1ProviderContentBuilder::new().with_committed([tx.clone()]).build_into_l1_provider();

    l1_provider
        .add_events(vec![Event::TransactionConsumed {
            tx_hash: tx.tx_hash,
            timestamp: BlockTimestamp(0),
        }])
        .unwrap();

    let expected = L1ProviderContentBuilder::new()
        .with_consumed_txs([ConsumedTransaction { tx: tx.clone(), timestamp: BlockTimestamp(0) }])
        .build();
    expected.assert_eq(&l1_provider);
}

#[test]
fn consuming_tx_marked_for_cancellation() {
    // Setup.
    let tx = l1_handler(1);
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_cancel_requested_txs([tx.clone()])
        .build_into_l1_provider();

    l1_provider
        .add_events(vec![Event::TransactionConsumed {
            tx_hash: tx.tx_hash,
            timestamp: BlockTimestamp(0),
        }])
        .unwrap();

    let expected = L1ProviderContentBuilder::new()
        .with_consumed_txs([ConsumedTransaction { tx: tx.clone(), timestamp: BlockTimestamp(0) }])
        .build();
    expected.assert_eq(&l1_provider);
}

#[test]
fn consuming_tx_cancelled_on_l2() {
    // Setup.
    let tx = l1_handler(1);
    let mut l1_provider =
        L1ProviderContentBuilder::new().with_cancelled_txs([tx.clone()]).build_into_l1_provider();

    l1_provider
        .add_events(vec![Event::TransactionConsumed {
            tx_hash: tx.tx_hash,
            timestamp: BlockTimestamp(0),
        }])
        .unwrap();

    let expected = L1ProviderContentBuilder::new()
        .with_consumed_txs([ConsumedTransaction { tx: tx.clone(), timestamp: BlockTimestamp(0) }])
        .build();
    expected.assert_eq(&l1_provider);
}

#[test]
fn consuming_pending_tx() {
    // Setup.
    let tx = l1_handler(1);
    let mut l1_provider =
        L1ProviderContentBuilder::new().with_txs([tx.clone()]).build_into_l1_provider();

    l1_provider
        .add_events(vec![Event::TransactionConsumed {
            tx_hash: tx.tx_hash,
            timestamp: BlockTimestamp(0),
        }])
        .unwrap();

    let expected = L1ProviderContentBuilder::new()
        .with_consumed_txs([ConsumedTransaction { tx: tx.clone(), timestamp: BlockTimestamp(0) }])
        .build();
    expected.assert_eq(&l1_provider);
}

#[test]
fn consuming_rejected_tx() {
    // Setup.
    let tx = l1_handler(1);
    let mut l1_provider =
        L1ProviderContentBuilder::new().with_rejected([tx.clone()]).build_into_l1_provider();

    l1_provider
        .add_events(vec![Event::TransactionConsumed {
            tx_hash: tx.tx_hash,
            timestamp: BlockTimestamp(0),
        }])
        .unwrap();

    let expected = L1ProviderContentBuilder::new()
        .with_consumed_txs([ConsumedTransaction { tx: tx.clone(), timestamp: BlockTimestamp(0) }])
        .build();
    expected.assert_eq(&l1_provider);
}

#[test]
#[should_panic]
fn consuming_consumed_tx_panics() {
    // Setup.
    let tx = l1_handler(1);
    let consumed_tx = ConsumedTransaction { tx: tx.clone(), timestamp: BlockTimestamp(0) };
    let timelock = 1000;
    let config = L1ProviderConfig {
        l1_handler_consumption_timelock_seconds: Duration::from_secs(timelock),
        ..Default::default()
    };
    let clock = Arc::new(FakeClock::new(5));
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_config(config)
        .with_clock(clock)
        .with_consumed_txs([consumed_tx])
        .build_into_l1_provider();

    assert!(l1_provider.tx_manager.records.contains_key(&tx.tx_hash));

    l1_provider
        .add_events(vec![Event::TransactionConsumed {
            tx_hash: tx.tx_hash,
            timestamp: BlockTimestamp(1),
        }])
        .unwrap();
}

#[test]
fn consuming_unkown_tx_does_not_change_the_provider_state() {
    // Setup.
    let cancellation_request_tx = l1_handler(1);
    let cancelled_on_l2_tx = l1_handler(2);
    let committed_tx = l1_handler(3);
    let rejected_tx = l1_handler(4);
    let pending_tx = l1_handler(5);
    let consumed_tx = ConsumedTransaction { tx: l1_handler(6), timestamp: BlockTimestamp(0) };
    let unknown_tx = l1_handler(7);

    let timelock = 1000;
    let config = L1ProviderConfig {
        l1_handler_consumption_timelock_seconds: Duration::from_secs(timelock),
        ..Default::default()
    };
    let clock = Arc::new(FakeClock::new(5));
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_config(config)
        .with_clock(clock)
        .with_cancel_requested_txs([cancellation_request_tx.clone()])
        .with_cancelled_txs([cancelled_on_l2_tx.clone()])
        .with_committed([committed_tx.clone()])
        .with_rejected([rejected_tx.clone()])
        .with_txs([pending_tx.clone()])
        .with_consumed_txs([consumed_tx.clone()])
        .build_into_l1_provider();

    l1_provider
        .add_events(vec![Event::TransactionConsumed {
            tx_hash: unknown_tx.tx_hash,
            timestamp: BlockTimestamp(0),
        }])
        .unwrap();

    let expected = L1ProviderContentBuilder::new()
        .with_cancel_requested_txs([cancellation_request_tx])
        .with_cancelled_txs([cancelled_on_l2_tx])
        .with_committed([committed_tx])
        .with_rejected([rejected_tx])
        .with_txs([pending_tx])
        .with_consumed_txs([consumed_tx])
        .build();
    expected.assert_eq(&l1_provider);
}

#[test]
fn consuming_tx_deletes_after_timelock() {
    // Setup.
    let tx = l1_handler(1);
    let dummy_tx = ConsumedTransaction { tx: l1_handler(999), timestamp: BlockTimestamp(1200) }; // tx to consume to trigger the timelock
    let timelock = 1000;
    let config = L1ProviderConfig {
        l1_handler_consumption_timelock_seconds: Duration::from_secs(timelock),
        ..Default::default()
    };
    let clock = Arc::new(FakeClock::new(5));

    // Creating a provider with a pending tx.

    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_config(config)
        .with_clock(clock.clone())
        .with_txs([tx.clone()])
        .build_into_l1_provider();

    // Marking the tx as consumed.

    l1_provider
        .add_events(vec![Event::TransactionConsumed {
            tx_hash: tx.tx_hash,
            timestamp: BlockTimestamp(0),
        }])
        .unwrap();

    // Assert it is marked as consumed, but not deleted.

    let l1_provider_with_consumed = L1ProviderContentBuilder::new()
        .with_consumed_txs([ConsumedTransaction { tx: tx.clone(), timestamp: BlockTimestamp(0) }])
        .build();

    l1_provider_with_consumed.assert_eq(&l1_provider);

    // Advance the clock and assert the tx is deleted.

    clock.advance(l1_provider.config.l1_handler_consumption_timelock_seconds);

    // Consume the dummy tx to trigger the deletion past the timelock.
    l1_provider
        .add_events(vec![Event::TransactionConsumed {
            tx_hash: dummy_tx.tx.tx_hash,
            timestamp: dummy_tx.timestamp,
        }])
        .unwrap();

    let l1_provider_with_consumed_deleted = L1ProviderContentBuilder::new().build();
    l1_provider_with_consumed_deleted.assert_eq(&l1_provider);
}

#[test]
fn consuming_multiple_txs_selective_deletion_after_timelock() {
    // Test that only transactions past the timelock are deleted, while newer ones remain
    // - Consume tx1 at timestamp 100
    // - Consume tx2 at timestamp 1000
    // - Set timelock to 500 and clock at 1200
    // - Verify tx1 is deleted but tx2 remains after consuming a dummy tx (to trigger the deletion)

    // Setup.
    let tx1 = l1_handler(1);
    let tx2 = l1_handler(2);
    let dummy_tx = ConsumedTransaction { tx: l1_handler(999), timestamp: BlockTimestamp(1200) }; // tx to consume to trigger the timelock
    let timelock = 500; // 500 seconds timelock
    let early_consumption_timestamp = 100;
    let late_consumption_timestamp = 1000;

    let config = L1ProviderConfig {
        l1_handler_consumption_timelock_seconds: Duration::from_secs(timelock),
        ..Default::default()
    };

    // Start time at 1200, which is past timelock for tx1 but not tx2
    // tx1 consumed at 100, timelock passes at 100 + 500 = 600
    // tx2 consumed at 1000, timelock passes at 1000 + 500 = 1500
    // So at time 1200, only tx1 should be deleted
    let clock = Arc::new(FakeClock::new(1200));

    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_config(config)
        .with_clock(clock.clone())
        .with_txs([dummy_tx.tx.clone()])
        .with_consumed_txs([
            ConsumedTransaction {
                tx: tx1.clone(),
                timestamp: BlockTimestamp(early_consumption_timestamp),
            },
            ConsumedTransaction {
                tx: tx2.clone(),
                timestamp: BlockTimestamp(late_consumption_timestamp),
            },
        ])
        .build_into_l1_provider();

    // Consume the dummy tx to trigger the deletion past the timelock.
    l1_provider
        .add_events(vec![Event::TransactionConsumed {
            tx_hash: dummy_tx.tx.tx_hash,
            timestamp: dummy_tx.timestamp,
        }])
        .unwrap();

    // Only tx2 should remain consumed, tx1 should be deleted
    let expected_with_tx1_deleted = L1ProviderContentBuilder::new()
        .with_consumed_txs([
            ConsumedTransaction { tx: dummy_tx.tx, timestamp: dummy_tx.timestamp },
            ConsumedTransaction {
                tx: tx2.clone(),
                timestamp: BlockTimestamp(late_consumption_timestamp),
            },
        ])
        .build();
    expected_with_tx1_deleted.assert_eq(&l1_provider);
}
