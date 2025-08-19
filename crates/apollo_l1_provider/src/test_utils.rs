use std::collections::BTreeMap;
use std::mem;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use apollo_l1_provider_types::{
    Event,
    L1ProviderClient,
    L1ProviderClientResult,
    L1ProviderSnapshot,
    SessionState,
    ValidationStatus,
};
use apollo_time::test_utils::FakeClock;
use apollo_time::time::{Clock, DefaultClock};
use async_trait::async_trait;
use indexmap::{IndexMap, IndexSet};
use itertools::{chain, Itertools};
use pretty_assertions::assert_eq;
use starknet_api::block::{BlockNumber, BlockTimestamp, UnixTimestamp};
use starknet_api::executable_transaction::{
    L1HandlerTransaction as ExecutableL1HandlerTransaction,
    L1HandlerTransaction,
};
use starknet_api::hash::StarkHash;
use starknet_api::test_utils::l1_handler::{executable_l1_handler_tx, L1HandlerTxArgs};
use starknet_api::transaction::TransactionHash;

use crate::bootstrapper::CommitBlockBacklog;
use crate::l1_provider::L1Provider;
use crate::transaction_manager::{StagingEpoch, TransactionManager, TransactionManagerConfig};
use crate::transaction_record::{TransactionPayload, TransactionRecord};
use crate::{L1ProviderConfig, ProviderState};

pub fn l1_handler(tx_hash: usize) -> L1HandlerTransaction {
    let tx_hash = TransactionHash(StarkHash::from(tx_hash));
    executable_l1_handler_tx(L1HandlerTxArgs { tx_hash, ..Default::default() })
}

// Represents the internal content of the L1 provider for testing.
// Enables customized (and potentially inconsistent) creation for unit testing.
#[derive(Debug, Default)]
pub struct L1ProviderContent {
    config: Option<L1ProviderConfig>,
    tx_manager_content: Option<TransactionManagerContent>,
    state: Option<ProviderState>,
    current_height: Option<BlockNumber>,
    clock: Option<Arc<dyn Clock>>,
}

impl L1ProviderContent {
    #[track_caller]
    pub fn assert_eq(&self, l1_provider: &L1Provider) {
        if let Some(tx_manager_content) = &self.tx_manager_content {
            tx_manager_content.assert_eq(&l1_provider.tx_manager);
        } else {
            assert!(l1_provider.tx_manager.snapshot().is_empty());
        }

        if let Some(state) = &self.state {
            assert_eq!(&l1_provider.state, state);
        }

        if let Some(current_height) = &self.current_height {
            assert_eq!(&l1_provider.current_height, current_height);
        }
    }
}

impl From<L1ProviderContent> for L1Provider {
    fn from(content: L1ProviderContent) -> L1Provider {
        L1Provider {
            config: content.config.unwrap_or_default(),
            tx_manager: content.tx_manager_content.map(Into::into).unwrap_or_default(),
            // Defaulting to Pending state, since a provider with a "default" Bootstrapper
            // is functionally equivalent to Pending for testing purposes.
            state: content.state.unwrap_or(ProviderState::Pending),
            current_height: content.current_height.unwrap_or_default(),
            start_height: content.current_height.unwrap_or_default(),
            clock: content.clock.unwrap_or_else(|| Arc::new(DefaultClock)),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct L1ProviderContentBuilder {
    config: Option<L1ProviderConfig>,
    tx_manager_content_builder: TransactionManagerContentBuilder,
    state: Option<ProviderState>,
    current_height: Option<BlockNumber>,
    clock: Option<Arc<dyn Clock>>,
}

impl L1ProviderContentBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_config(mut self, config: L1ProviderConfig) -> Self {
        self.config = Some(config);
        self
    }

    pub fn with_state(mut self, state: ProviderState) -> Self {
        self.state = Some(state);
        self
    }

    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = Some(clock);
        self
    }

    pub fn with_txs(mut self, txs: impl IntoIterator<Item = L1HandlerTransaction>) -> Self {
        self.tx_manager_content_builder = self.tx_manager_content_builder.with_txs(txs);
        self
    }

    pub fn with_timed_txs(
        mut self,
        txs: impl IntoIterator<Item = (L1HandlerTransaction, u64)>,
    ) -> Self {
        let timed_txs = txs.into_iter().map(Into::into);
        self.tx_manager_content_builder = self.tx_manager_content_builder.with_timed_txs(timed_txs);
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

    pub fn with_committed(
        mut self,
        committed: impl IntoIterator<Item = L1HandlerTransaction>,
    ) -> Self {
        self.tx_manager_content_builder = self.tx_manager_content_builder.with_committed(committed);
        self
    }

    pub fn with_timed_committed(
        mut self,
        committed: impl IntoIterator<Item = (L1HandlerTransaction, u64)>,
    ) -> Self {
        let committed = committed.into_iter().map(Into::into);
        self.tx_manager_content_builder =
            self.tx_manager_content_builder.with_timed_committed(committed);
        self
    }

    pub fn with_committed_hashes(
        mut self,
        tx_hashes: impl IntoIterator<Item = TransactionHash>,
    ) -> Self {
        self.tx_manager_content_builder =
            self.tx_manager_content_builder.with_committed_hashes(tx_hashes);
        self
    }

    pub fn with_consumed(
        mut self,
        consumed: impl IntoIterator<Item = L1HandlerTransaction>,
    ) -> Self {
        self.tx_manager_content_builder = self.tx_manager_content_builder.with_consumed(consumed);
        self
    }

    pub fn with_timed_cancel_requested_txs(
        mut self,
        cancel_requested: impl IntoIterator<Item = (L1HandlerTransaction, u64)>,
    ) -> Self {
        let cancel_requested = cancel_requested.into_iter().map(Into::into);
        self.tx_manager_content_builder =
            self.tx_manager_content_builder.with_cancel_requested_txs(cancel_requested);
        self
    }

    pub fn with_cancel_requested_txs(
        mut self,
        cancel_requested: impl IntoIterator<Item = L1HandlerTransaction>,
    ) -> Self {
        self = self.with_nonzero_timelock_setup();

        let now = self.clock.as_ref().unwrap().unix_now();
        let cancellation_request_timestamp = now;
        let cancel_requested =
            cancel_requested.into_iter().map(|tx| (tx, cancellation_request_timestamp));
        self.with_timed_cancel_requested_txs(cancel_requested)
    }

    pub fn with_cancelled_txs(
        mut self,
        cancelled: impl IntoIterator<Item = L1HandlerTransaction>,
    ) -> Self {
        self = self.with_nonzero_timelock_setup();

        let now = self.clock.as_ref().unwrap().unix_now();
        let cancellation_timelock =
            self.config.unwrap().l1_handler_cancellation_timelock_seconds.as_secs();
        // If a tx's timestamp is OLDER than the timelock, then it's timeout is expired and it's
        // considered fully cancelled on L2.
        let cancellation_expired = now - (cancellation_timelock + 1);
        let cancelled = cancelled.into_iter().map(|tx| (tx, cancellation_expired));

        self.with_timed_cancel_requested_txs(cancelled)
    }

    /// Use to test timelocking of new l1-handler transactions, if you don't care about the actual
    /// timestamp values. If you want to test specific timestamp values, use `with_timed_txs` and
    /// set clock and cooldown configs manually through the setters.
    /// Note: do not set clock/configs manually if you use this method, or you may get unexpected
    /// results.
    pub fn with_timelocked_txs(
        mut self,
        txs: impl IntoIterator<Item = L1HandlerTransaction>,
    ) -> Self {
        self = self.with_nonzero_timelock_setup();

        let now = self.clock.as_ref().unwrap().unix_now();
        // An l1-handler is timelocked if if was created less than `cooldown` seconds ago. Since
        // timelock is nonzero, all txs created `now` are trivially timelocked.
        let timelocked_tx_timestamp = now;
        let txs =
            txs.into_iter().map(|tx| (tx, timelocked_tx_timestamp)).map(Into::into).collect_vec();

        self.tx_manager_content_builder = self.tx_manager_content_builder.with_timed_txs(txs);
        self
    }

    pub fn build(mut self) -> L1ProviderContent {
        if let Some(config) = self.config {
            self.tx_manager_content_builder =
                self.tx_manager_content_builder.with_config(config.into());
        }

        L1ProviderContent {
            config: self.config,
            tx_manager_content: self.tx_manager_content_builder.build(),
            state: self.state,
            current_height: self.current_height,
            clock: self.clock,
        }
    }

    pub fn build_into_l1_provider(self) -> L1Provider {
        self.build().into()
    }

    fn with_nonzero_timelock_setup(mut self) -> Self {
        let base_timestamp = 5; // Arbitrary small base timestamp.
        self.clock = self.clock.take().or_else(|| Some(Arc::new(FakeClock::new(base_timestamp))));

        let nonzero_timelock = Duration::from_secs(1);
        let config = self.config.unwrap_or_default();
        self.with_config(L1ProviderConfig {
            new_l1_handler_cooldown_seconds: nonzero_timelock,
            l1_handler_cancellation_timelock_seconds: nonzero_timelock,
            ..config
        })
    }
}

// Represents the internal content of the TransactionManager.
// Enables customized (and potentially inconsistent) creation for unit testing.
#[derive(Debug, Default)]
struct TransactionManagerContent {
    pub uncommitted: Option<Vec<TimedL1HandlerTransaction>>,
    pub rejected: Option<Vec<L1HandlerTransaction>>,
    pub committed: Option<IndexMap<TransactionHash, TransactionPayload>>,
    pub consumed: Option<IndexMap<TransactionHash, TransactionPayload>>,
    pub cancel_requested: Option<Vec<CancellationRequest>>,
    pub config: Option<TransactionManagerConfig>,
}

impl TransactionManagerContent {
    #[track_caller]
    fn assert_eq(&self, tx_manager: &TransactionManager) {
        let snapshot = tx_manager.snapshot();

        if let Some(uncommitted) = &self.uncommitted {
            assert_eq!(
                uncommitted
                    .iter()
                    .map(|TimedL1HandlerTransaction { tx, .. }| tx.tx_hash)
                    .collect_vec(),
                snapshot.uncommitted
            );
        }

        if let Some(expected_committed) = &self.committed {
            assert_eq!(expected_committed.keys().copied().collect_vec(), snapshot.committed);
        }

        if let Some(rejected) = &self.rejected {
            assert_eq!(rejected.iter().map(|tx| tx.tx_hash).collect_vec(), snapshot.rejected);
        }

        if let Some(cancel_requested) = &self.cancel_requested {
            assert_eq!(
                cancel_requested
                    .iter()
                    .map(|CancellationRequest { tx, .. }| tx.tx_hash)
                    .collect_vec(),
                chain!(snapshot.cancellation_started_on_l2, snapshot.cancelled_on_l2).collect_vec(),
            );
        }
    }
}

impl From<TransactionManagerContent> for TransactionManager {
    fn from(mut content: TransactionManagerContent) -> TransactionManager {
        let pending: Vec<_> = mem::take(&mut content.uncommitted).unwrap_or_default();
        let rejected: Vec<_> = mem::take(&mut content.rejected).unwrap_or_default();
        let committed: IndexMap<_, _> = mem::take(&mut content.committed).unwrap_or_default();
        let consumed: IndexMap<_, _> = mem::take(&mut content.consumed).unwrap_or_default();
        let cancel_requested: Vec<_> = mem::take(&mut content.cancel_requested).unwrap_or_default();

        let mut records = IndexMap::with_capacity(
            pending.len() + rejected.len() + committed.len() + cancel_requested.len(),
        );

        let mut proposable_index: BTreeMap<UnixTimestamp, Vec<TransactionHash>> = BTreeMap::new();
        for timed_tx in pending {
            let tx_hash = timed_tx.tx.tx_hash;
            let block_timestamp = timed_tx.timestamp;
            let record = TransactionRecord::from(timed_tx);
            assert_eq!(records.insert(tx_hash, record), None);
            proposable_index.entry(block_timestamp.0).or_default().push(tx_hash);
        }

        for rejected_tx in rejected {
            let tx_hash = rejected_tx.tx_hash;
            let mut record = TransactionRecord::new(TransactionPayload::Full {
                tx: rejected_tx,
                created_at_block_timestamp: 0.into(), /* timestamps are irrelevant for txs once
                                                       * rejected. */
                scrape_timestamp: 0,
            });
            record.mark_rejected();
            assert_eq!(records.insert(tx_hash, record), None);
        }

        for (tx_hash, committed_tx) in committed {
            let mut record = TransactionRecord::from(committed_tx);
            record.mark_committed();
            assert_eq!(records.insert(tx_hash, record), None);
        }

        for cancel_requested_tx in cancel_requested {
            let tx_hash = cancel_requested_tx.tx.tx_hash;
            let mut record = TransactionRecord::new(TransactionPayload::Full {
                tx: cancel_requested_tx.tx,
                // Transaction "created_at" irrelevant after cancellation request.
                created_at_block_timestamp: 0.into(),
                scrape_timestamp: 0,
            });
            record.mark_cancellation_request(cancel_requested_tx.timestamp);
            assert_eq!(records.insert(tx_hash, record), None);
        }

        for (tx_hash, consumed_tx) in consumed {
            let mut record = TransactionRecord::from(consumed_tx);
            record.mark_committed();
            record.mark_consumed(0.into());
            assert_eq!(records.insert(tx_hash, record), None);
        }

        let current_epoch = StagingEpoch::new();
        TransactionManager::create_for_testing(
            records.into(),
            proposable_index,
            current_epoch,
            content.config.unwrap_or_default(),
        )
    }
}

#[derive(Clone, Debug, Default)]
struct TransactionManagerContentBuilder {
    uncommitted: Option<Vec<TimedL1HandlerTransaction>>,
    rejected: Option<Vec<L1HandlerTransaction>>,
    committed: Option<IndexMap<TransactionHash, TransactionPayload>>,
    consumed: Option<IndexMap<TransactionHash, TransactionPayload>>,
    config: Option<TransactionManagerConfig>,
    cancel_requested: Option<Vec<CancellationRequest>>,
}

impl TransactionManagerContentBuilder {
    fn with_txs(mut self, txs: impl IntoIterator<Item = L1HandlerTransaction>) -> Self {
        let dummy_timestamp = 0;
        self.uncommitted
            .get_or_insert_default()
            .extend(txs.into_iter().map(|tx| (tx, dummy_timestamp).into()).collect_vec());
        self
    }

    fn with_timed_txs(mut self, txs: impl IntoIterator<Item = TimedL1HandlerTransaction>) -> Self {
        self.uncommitted.get_or_insert_default().extend(txs.into_iter().collect_vec());
        self
    }

    fn with_rejected(mut self, rejected: impl IntoIterator<Item = L1HandlerTransaction>) -> Self {
        self.rejected = Some(rejected.into_iter().collect());
        self
    }

    fn with_committed(mut self, committed: impl IntoIterator<Item = L1HandlerTransaction>) -> Self {
        self.committed.get_or_insert_default().extend(
            committed
                .into_iter()
                // created at block is irrelevant for committed txs.
                .map(|tx| (tx.tx_hash, TransactionPayload::Full { tx, created_at_block_timestamp: 0.into(), scrape_timestamp: 0 })),
        );
        self
    }

    fn with_timed_committed(
        mut self,
        committed: impl IntoIterator<Item = TimedL1HandlerTransaction>,
    ) -> Self {
        self.committed.get_or_insert_default().extend(committed.into_iter().map(|timed_tx| {
            (
                timed_tx.tx.tx_hash,
                TransactionPayload::Full {
                    tx: timed_tx.tx,
                    created_at_block_timestamp: timed_tx.timestamp,
                    scrape_timestamp: timed_tx.timestamp.0,
                },
            )
        }));
        self
    }

    fn with_committed_hashes(
        mut self,
        committed_hashes: impl IntoIterator<Item = TransactionHash>,
    ) -> Self {
        self.committed.get_or_insert_default().extend(
            committed_hashes
                .into_iter()
                .map(|tx_hash| (tx_hash, TransactionPayload::HashOnly(tx_hash))),
        );
        self
    }

    fn with_consumed(mut self, consumed: impl IntoIterator<Item = L1HandlerTransaction>) -> Self {
        self.consumed.get_or_insert_default().extend(consumed.into_iter().map(|tx| {
            (
                tx.tx_hash,
                TransactionPayload::Full {
                    tx,
                    created_at_block_timestamp: 0.into(),
                    scrape_timestamp: 0,
                },
            )
        }));
        self
    }

    pub fn with_cancel_requested_txs(
        mut self,
        cancel_requested: impl IntoIterator<Item = CancellationRequest>,
    ) -> Self {
        self.cancel_requested = Some(cancel_requested.into_iter().collect());
        self
    }

    fn with_config(mut self, config: TransactionManagerConfig) -> Self {
        self.config = Some(config);
        self
    }

    fn build(self) -> Option<TransactionManagerContent> {
        if self.is_default() {
            return None;
        }

        Some(TransactionManagerContent {
            uncommitted: self.uncommitted,
            committed: self.committed,
            consumed: self.consumed,
            rejected: self.rejected,
            cancel_requested: self.cancel_requested,
            config: self.config,
        })
    }

    fn is_default(&self) -> bool {
        self.uncommitted.is_none() && self.committed.is_none() && self.cancel_requested.is_none()
    }
}

/// A fake L1 provider client that buffers all received messages, allow asserting the order in which
/// they were received, and forward them to the l1 provider (flush the messages).
#[derive(Default)]
pub struct FakeL1ProviderClient {
    // Interior mutability needed since this is modifying during client API calls, which are all
    // immutable.
    pub events_received: Mutex<Vec<Event>>,
    pub commit_blocks_received: Mutex<Vec<CommitBlockBacklog>>,
}

impl FakeL1ProviderClient {
    /// Apply all messages received to the l1 provider.
    pub async fn flush_messages(&self, l1_provider: &mut L1Provider) {
        let commit_blocks = self.commit_blocks_received.lock().unwrap().drain(..).collect_vec();
        for CommitBlockBacklog { height, committed_txs } in commit_blocks {
            l1_provider.commit_block(committed_txs, [].into(), height).unwrap();
        }

        // TODO(gilad): flush other buffers if necessary.
    }

    #[track_caller]
    pub fn assert_add_events_received_with(&self, expected: &[Event]) {
        let events_received = mem::take(&mut *self.events_received.lock().unwrap());
        assert_eq!(events_received, expected);
    }
}

#[async_trait]
impl L1ProviderClient for FakeL1ProviderClient {
    async fn start_block(
        &self,
        _state: SessionState,
        _height: BlockNumber,
    ) -> L1ProviderClientResult<()> {
        todo!()
    }

    async fn get_txs(
        &self,
        _n_txs: usize,
        _height: BlockNumber,
    ) -> L1ProviderClientResult<Vec<ExecutableL1HandlerTransaction>> {
        todo!()
    }

    async fn add_events(&self, events: Vec<Event>) -> L1ProviderClientResult<()> {
        self.events_received.lock().unwrap().extend(events);
        Ok(())
    }

    async fn commit_block(
        &self,
        l1_handler_tx_hashes: IndexSet<TransactionHash>,
        _rejected_l1_handler_tx_hashes: IndexSet<TransactionHash>,
        height: BlockNumber,
    ) -> L1ProviderClientResult<()> {
        self.commit_blocks_received
            .lock()
            .unwrap()
            .push(CommitBlockBacklog { height, committed_txs: l1_handler_tx_hashes });
        Ok(())
    }

    async fn validate(
        &self,
        _tx_hash: TransactionHash,
        _height: BlockNumber,
    ) -> L1ProviderClientResult<ValidationStatus> {
        todo!()
    }

    async fn initialize(&self, _events: Vec<Event>) -> L1ProviderClientResult<()> {
        todo!()
    }

    async fn get_l1_provider_snapshot(&self) -> L1ProviderClientResult<L1ProviderSnapshot> {
        todo!()
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct TimedL1HandlerTransaction {
    pub tx: L1HandlerTransaction,
    pub timestamp: BlockTimestamp,
}

impl From<(L1HandlerTransaction, u64)> for TimedL1HandlerTransaction {
    fn from((tx, timestamp): (L1HandlerTransaction, u64)) -> Self {
        Self { timestamp: timestamp.into(), tx }
    }
}

impl From<TimedL1HandlerTransaction> for TransactionRecord {
    fn from(timed_tx: TimedL1HandlerTransaction) -> Self {
        TransactionRecord::new(TransactionPayload::Full {
            tx: timed_tx.tx,
            created_at_block_timestamp: timed_tx.timestamp,
            scrape_timestamp: timed_tx.timestamp.0,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CancellationRequest {
    pub tx: L1HandlerTransaction,
    pub timestamp: BlockTimestamp,
}

impl From<(L1HandlerTransaction, u64)> for CancellationRequest {
    fn from((tx, timestamp): (L1HandlerTransaction, u64)) -> Self {
        Self { tx, timestamp: timestamp.into() }
    }
}
