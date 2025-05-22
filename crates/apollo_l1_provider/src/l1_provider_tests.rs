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
use assert_matches::assert_matches;
use itertools::Itertools;
use pretty_assertions::assert_eq;
use rstest::rstest;
use starknet_api::block::BlockNumber;
use starknet_api::test_utils::l1_handler::{executable_l1_handler_tx, L1HandlerTxArgs};
use starknet_api::transaction::TransactionHash;
use starknet_api::tx_hash;

use crate::bootstrapper::{Bootstrapper, CommitBlockBacklog, SyncTaskHandle};
use crate::l1_provider::L1Provider;
use crate::test_utils::{l1_handler, FakeL1ProviderClient, L1ProviderContentBuilder};
use crate::ProviderState;

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

fn l1_handler_event(tx_hash: TransactionHash) -> Event {
    Event::L1HandlerTransaction(executable_l1_handler_tx(L1HandlerTxArgs {
        tx_hash,
        ..Default::default()
    }))
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
        .with_committed([tx_hash!(2)])
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
    assert_eq!(
        l1_provider.validate(tx_hash!(3), BlockNumber(0)).unwrap(),
        ValidationStatus::Invalid(InvalidValidationStatus::ConsumedOnL1OrUnknown)
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
            .with_committed(vec![])
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
            .with_committed(vec![])
            // State should be unchanged.
            .with_state(state)
            .build();

        expected_l1_provider.assert_eq(&l1_provider);
    }
}

#[test]
fn process_events_committed_txs() {
    // Setup.
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_txs([l1_handler(1)])
        .with_committed(vec![tx_hash!(2)])
        .with_state(ProviderState::Pending)
        .build_into_l1_provider();

    let expected_l1_provider = l1_provider.clone();

    // Test.
    // Uncommitted transaction, should fail silently.
    l1_provider.add_events(vec![l1_handler_event(tx_hash!(1))]).unwrap();
    assert_eq!(l1_provider, expected_l1_provider);

    // Committed transaction, should fail silently.
    l1_provider.add_events(vec![l1_handler_event(tx_hash!(2))]).unwrap();
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

#[test]
fn commit_block_backlog() {
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
fn tx_in_commit_block_before_processed_is_skipped() {
    // Setup
    let mut l1_provider =
        L1ProviderContentBuilder::new().with_committed([tx_hash!(1)]).build_into_l1_provider();

    // Transactions unknown yet.
    commit_block_no_rejected(&mut l1_provider, &[tx_hash!(2), tx_hash!(3)], BlockNumber(0));
    let expected_l1_provider = L1ProviderContentBuilder::new()
        .with_committed([tx_hash!(1), tx_hash!(2), tx_hash!(3)])
        .build();
    expected_l1_provider.assert_eq(&l1_provider);

    // Parsing the tx after getting it from commit-block is a NOP.
    l1_provider.add_events(vec![l1_handler_event(tx_hash!(2))]).unwrap();
    expected_l1_provider.assert_eq(&l1_provider);
}

#[test]
fn bootstrap_commit_block_received_twice_no_error() {
    // Setup.
    let initial_bootstrap_state = ProviderState::Bootstrap(bootstrapper!(
        backlog: [],
        catch_up: 2
    ));
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_txs([l1_handler(1), l1_handler(2)])
        .with_state(initial_bootstrap_state)
        .build_into_l1_provider();

    // Test.
    commit_block_no_rejected(&mut l1_provider, &[tx_hash!(1)], BlockNumber(0));
    // No error, since the this tx hash is already known to be committed.
    commit_block_no_rejected(&mut l1_provider, &[tx_hash!(1)], BlockNumber(0));
}

#[test]
fn bootstrap_commit_block_received_twice_error_if_new_uncommitted_txs() {
    // Setup.
    let initial_bootstrap_state = ProviderState::Bootstrap(bootstrapper!(
        backlog: [],
        catch_up: 2
    ));
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_txs([l1_handler(1), l1_handler(2)])
        .with_state(initial_bootstrap_state)
        .build_into_l1_provider();

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
        .with_committed([tx_hash!(2)])
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
        .with_committed([tx_hash!(2)])
        .with_height(BlockNumber(1))
        .build();
    expected_l1_provider.assert_eq(&l1_provider);

    // Ensure that the rejected transaction is not re-added to the provider, even if it is staged.
    l1_provider.validate(rejected_tx_id, BlockNumber(1)).unwrap();
    l1_provider.add_events(vec![l1_handler_event(rejected_tx_id)]).unwrap();
    expected_l1_provider.assert_eq(&l1_provider);
}
