use std::collections::{BTreeMap, HashSet};
use std::mem;
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
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::test_utils::l1_handler::{executable_l1_handler_tx, L1HandlerTxArgs};
use starknet_api::transaction::TransactionHash;
use starknet_api::tx_hash;

use crate::bootstrapper::{Bootstrapper, CommitBlockBacklog, SyncTaskHandle};
use crate::l1_provider::L1Provider;
use crate::soft_delete_index_map::SoftDeleteIndexMap;
use crate::test_utils::{l1_handler, FakeL1ProviderClient};
use crate::transaction_manager::TransactionManager;
use crate::{L1ProviderConfig, ProviderState};

// Represents the internal content of the L1 provider for testing.
// Enables customized (and potentially inconsistent) creation for unit testing.
#[derive(Default)]
pub struct L1ProviderContent {
    tx_manager_content: Option<TransactionManagerContent>,
    state: Option<ProviderState>,
    current_height: Option<BlockNumber>,
    cancellation_requests: Option<BTreeMap<BlockNumber, Vec<TransactionHash>>>,
    config_cancellation_timelock_in_blocks: Option<BlockNumber>,
}

impl L1ProviderContent {
    #[track_caller]
    pub fn assert_eq(&self, l1_provider: &L1Provider) {
        if let Some(tx_manager_content) = &self.tx_manager_content {
            tx_manager_content.assert_eq(&l1_provider.tx_manager);
        }

        if let Some(state) = &self.state {
            assert_eq!(&l1_provider.state, state);
        }

        if let Some(current_height) = &self.current_height {
            assert_eq!(&l1_provider.current_height, current_height);
        }

        if let Some(cancellation_requests) = &self.cancellation_requests {
            assert_eq!(&l1_provider.cancellation_requests, cancellation_requests);
        }

        if let Some(config_cancellation_timelock_in_blocks) =
            &self.config_cancellation_timelock_in_blocks
        {
            assert_eq!(
                &l1_provider.config.cancellation_timelock_in_blocks,
                config_cancellation_timelock_in_blocks
            );
        }
    }
}

impl From<L1ProviderContent> for L1Provider {
    fn from(content: L1ProviderContent) -> L1Provider {
        L1Provider {
            config: L1ProviderConfig {
                cancellation_timelock_in_blocks: content
                    .config_cancellation_timelock_in_blocks
                    .unwrap_or_default(),
                ..Default::default()
            },
            tx_manager: content.tx_manager_content.map(Into::into).unwrap_or_default(),
            // Defaulting to Pending state, since a provider with a "default" Bootstrapper
            // is functionally equivalent to Pending for testing purposes.
            state: content.state.unwrap_or(ProviderState::Pending),
            current_height: content.current_height.unwrap_or_default(),
            cancellation_requests: content.cancellation_requests.unwrap_or_default(),
        }
    }
}

#[derive(Debug, Default)]
pub struct L1ProviderContentBuilder {
    tx_manager_content_builder: TransactionManagerContentBuilder,
    state: Option<ProviderState>,
    current_height: Option<BlockNumber>,
    cancellation_requests: Option<BTreeMap<BlockNumber, Vec<TransactionHash>>>,
    config_cancellation_timelock_in_blocks: Option<BlockNumber>,
}

impl L1ProviderContentBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_state(mut self, state: ProviderState) -> Self {
        self.state = Some(state);
        self
    }

    pub fn with_txs(mut self, txs: impl IntoIterator<Item = L1HandlerTransaction>) -> Self {
        self.tx_manager_content_builder = self.tx_manager_content_builder.with_txs(txs);
        self
    }

    pub fn with_rejected(mut self, txs: impl IntoIterator<Item = L1HandlerTransaction>) -> Self {
        self.tx_manager_content_builder = self.tx_manager_content_builder.with_rejected(txs);
        self
    }

    pub fn with_height(mut self, height: BlockNumber) -> Self {
        self.current_height = Some(height);
        self
    }

    pub fn with_committed(mut self, tx_hashes: impl IntoIterator<Item = TransactionHash>) -> Self {
        self.tx_manager_content_builder = self.tx_manager_content_builder.with_committed(tx_hashes);
        self
    }

    // At the time of writing, this is the only interesting part of the config for tests, if this
    // changes replace this with a `with_config` method.
    pub fn with_config_cancellation_timelock_config(
        mut self,
        cancellation_timelock_in_blocks: BlockNumber,
    ) -> Self {
        self.config_cancellation_timelock_in_blocks = Some(cancellation_timelock_in_blocks);
        self
    }

    pub fn with_cancellation_requests(
        mut self,
        cancellation_requests: impl IntoIterator<Item = (BlockNumber, Vec<TransactionHash>)>,
    ) -> Self {
        self.cancellation_requests = Some(cancellation_requests.into_iter().collect());
        self
    }

    pub fn build(self) -> L1ProviderContent {
        L1ProviderContent {
            tx_manager_content: self.tx_manager_content_builder.build(),
            state: self.state,
            current_height: self.current_height,
            cancellation_requests: self.cancellation_requests,
            config_cancellation_timelock_in_blocks: self.config_cancellation_timelock_in_blocks,
        }
    }

    pub fn build_into_l1_provider(self) -> L1Provider {
        self.build().into()
    }
}

// Represents the internal content of the TransactionManager.
// Enables customized (and potentially inconsistent) creation for unit testing.
#[derive(Debug, Default)]
struct TransactionManagerContent {
    pub uncommitted: Option<Vec<L1HandlerTransaction>>,
    pub rejected: Option<Vec<L1HandlerTransaction>>,
    pub committed: Option<HashSet<TransactionHash>>,
}

impl TransactionManagerContent {
    #[track_caller]
    fn assert_eq(&self, tx_manager: &TransactionManager) {
        if let Some(uncommitted) = &self.uncommitted {
            assert_eq!(
                uncommitted,
                &tx_manager.uncommitted.txs.values().map(|tx| tx.transaction.clone()).collect_vec()
            );
        }

        if let Some(committed) = &self.committed {
            assert_eq!(committed, &tx_manager.committed);
        }

        if let Some(rejected) = &self.rejected {
            assert_eq!(
                rejected,
                &tx_manager.rejected.txs.values().map(|tx| tx.transaction.clone()).collect_vec()
            );
        }
    }
}

impl From<TransactionManagerContent> for TransactionManager {
    fn from(mut content: TransactionManagerContent) -> TransactionManager {
        let txs: Vec<_> = mem::take(&mut content.uncommitted).unwrap_or_default();
        TransactionManager {
            uncommitted: SoftDeleteIndexMap::from(txs),
            committed: content.committed.unwrap_or_default(),
            rejected: SoftDeleteIndexMap::default(),
        }
    }
}

#[derive(Debug, Default)]
struct TransactionManagerContentBuilder {
    uncommitted: Option<Vec<L1HandlerTransaction>>,
    rejected: Option<Vec<L1HandlerTransaction>>,
    committed: Option<HashSet<TransactionHash>>,
}

impl TransactionManagerContentBuilder {
    fn with_txs(mut self, txs: impl IntoIterator<Item = L1HandlerTransaction>) -> Self {
        self.uncommitted = Some(txs.into_iter().collect());
        self
    }

    fn with_rejected(mut self, rejected: impl IntoIterator<Item = L1HandlerTransaction>) -> Self {
        self.rejected = Some(rejected.into_iter().collect());
        self
    }

    fn with_committed(mut self, tx_hashes: impl IntoIterator<Item = TransactionHash>) -> Self {
        self.committed = Some(tx_hashes.into_iter().collect());
        self
    }

    fn build(self) -> Option<TransactionManagerContent> {
        if self.is_default() {
            return None;
        }

        Some(TransactionManagerContent {
            uncommitted: self.uncommitted,
            committed: self.committed,
            rejected: self.rejected,
        })
    }

    fn is_default(&self) -> bool {
        self.uncommitted.is_none() && self.committed.is_none()
    }
}

fn setup_rejected_transactions() -> super::L1Provider {
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
    let rejected_txs = HashSet::from([tx1]);
    l1_provider.commit_block(&consumed_txs, &rejected_txs, first_block_number).unwrap();

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
                    committed_txs: vec![$(tx_hash!($tx)),*]
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
    l1_provider.commit_block(&[], &HashSet::new(), BlockNumber(10)).unwrap();

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
    l1_provider.commit_block(&[tx_hash!(1)], &HashSet::new(), BlockNumber(5)).unwrap();

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
    l1_provider.commit_block(&[tx_hash!(2)], &HashSet::new(), BlockNumber(5)).unwrap();

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

    l1_provider.commit_block(&[tx_hash!(3)], &HashSet::new(), BlockNumber(5)).unwrap();
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
    l1_provider.commit_block(&[tx_hash!(1)], &HashSet::new(), BlockNumber(8)).unwrap();
    let expected_l1_provider = L1ProviderContentBuilder::new()
        .with_txs([l1_handler(2), l1_handler(4)])
        .with_height(BlockNumber(9))
        .with_state(initial_bootstrap_state)
        .build();
    expected_l1_provider.assert_eq(&l1_provider);

    // Backlog is consumed, bootstrapping complete.
    l1_provider.commit_block(&[], &HashSet::new(), BlockNumber(9)).unwrap();
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
    l1_provider.commit_block(&[tx_hash!(2), tx_hash!(3)], &HashSet::new(), BlockNumber(0)).unwrap();
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
    l1_provider.commit_block(&[tx_hash!(1)], &HashSet::new(), BlockNumber(0)).unwrap();
    // No error, since the this tx hash is already known to be committed.
    l1_provider.commit_block(&[tx_hash!(1)], &HashSet::new(), BlockNumber(0)).unwrap();
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
    l1_provider.commit_block(&[tx_hash!(1)], &HashSet::new(), BlockNumber(0)).unwrap();
    // Error, since the new tx hash is not known to be committed.
    assert_matches!(
        l1_provider
            .commit_block(&[tx_hash!(1), tx_hash!(3)], &HashSet::new(), BlockNumber(0))
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
