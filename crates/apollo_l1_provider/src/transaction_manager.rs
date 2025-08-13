use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::ops::{Deref, Sub};
use std::time::Duration;

use apollo_l1_provider_types::{InvalidValidationStatus, ValidationStatus};
use starknet_api::block::BlockTimestamp;
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::transaction::TransactionHash;

use crate::transaction_record::{
    Records,
    TransactionPayload,
    TransactionRecord,
    TransactionRecordPolicy,
    TransactionState,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransactionManager {
    /// Storage of all l1 handler transactions --- keeps transactions until they can be safely
    /// removed, like when they are consumed on L1, or fully cancelled on L1.
    pub records: Records,
    pub config: TransactionManagerConfig,
    /// Ordered lexicographically by block timestamp, then order-of-arrival for
    /// identical timestamps, also at any point the staged transactions are a prefix of the
    /// structure under this order.
    /// Invariant: contains all hashes of transactions that are proposable, and only them.
    /// Invarariant 2: Once removed from this index, a transaction will never be proposed again.
    proposable_index: BTreeMap<BlockTimestamp, Vec<TransactionHash>>,
    /// Generation counter used to prevent double usage of an l1 handler transaction in a single
    /// block.
    /// Calling `get_txs` or `validate_tx` tags the touched transactions with the current block
    /// counter, so that further calls will know not to touch them again.
    /// At the start and end (commit) of every block, the counter is incremented, thus "unstaging"
    /// all tagged transactions from the previous block attempt.
    // TODO(Gilad): remove "for rejected" from name when uncommitted is migrated to records DS.
    current_staging_epoch: StagingEpoch,
    /// All consumed transactions that are waiting to be removed from the transaction manager.
    /// Invariant: Ordered lexicographically by block timestamp, then order-of-arrival for
    /// identical timestamps.
    /// Invariant 2: A transaction is in the queue iff it is in the records and marked as consumed.
    consumed_queue: BTreeMap<BlockTimestamp, Vec<TransactionHash>>,
}

impl TransactionManager {
    pub fn new(
        new_l1_handler_tx_cooldown_secs: Duration,
        l1_handler_cancellation_timelock_seconds: Duration,
        l1_handler_consumption_timelock_seconds: Duration,
    ) -> Self {
        Self {
            config: TransactionManagerConfig {
                new_l1_handler_tx_cooldown_secs,
                l1_handler_cancellation_timelock_seconds,
                l1_handler_consumption_timelock_seconds,
            },
            records: Default::default(),
            proposable_index: Default::default(),
            current_staging_epoch: StagingEpoch::new(),
            consumed_queue: Default::default(),
        }
    }

    pub fn start_block(&mut self) {
        self.rollback_staging();
    }

    pub fn get_txs(&mut self, n_txs: usize, now: u64) -> Vec<L1HandlerTransaction> {
        // Oldest        Now.sub(timelock)     Newest       Now
        //  |<---  passed  --->|                 |           |
        //  |<--- cooldown --->|                 |           |
        // t-------------------------------------------------->
        let cutoff = now.saturating_sub(self.config.new_l1_handler_tx_cooldown_secs.as_secs());
        let past_cooldown_txs = self.proposable_index.range(..BlockTimestamp(cutoff));

        // Linear scan, but we expect this to be a small number of transactions (< 10 roughly).
        let unstaged_tx_hashes: Vec<_> = past_cooldown_txs
            .flat_map(|(_timestamp, tx_hashes)| tx_hashes.iter())
            .skip_while(|&&tx_hash| self.is_staged(tx_hash))
            .take(n_txs)
            .copied()
            .collect();

        let mut txs = Vec::with_capacity(n_txs);
        let current_staging_epoch = self.current_staging_epoch; // borrow-checker constraint.
        for tx_hash in unstaged_tx_hashes {
            let newly_staged =
                self.with_record(tx_hash, |record| record.try_mark_staged(current_staging_epoch));
            assert_eq!(
                newly_staged,
                Some(true),
                "Inconsistent storage state: indexed l1 handler {tx_hash} is not in storage or \
                 wasn't marked as staged."
            );

            txs.push(self.records[&tx_hash].get_unchecked().clone());
        }
        txs
    }

    pub fn validate_tx(&mut self, tx_hash: TransactionHash, unix_now: u64) -> ValidationStatus {
        let current_staging_epoch_cloned = self.current_staging_epoch;

        let policy = TransactionRecordPolicy {
            cancellation_timelock: self.config.l1_handler_cancellation_timelock_seconds,
        };

        let validation_status = self.with_record(tx_hash, |record| {
            // If the current time affects the state, update state now.
            record.update_time_based_state(unix_now, policy);

            if !record.is_validatable() {
                match record.state {
                    TransactionState::Committed => {
                        InvalidValidationStatus::AlreadyIncludedOnL2.into()
                    }
                    TransactionState::CancelledOnL2 => {
                        InvalidValidationStatus::CancelledOnL2.into()
                    }
                    TransactionState::Consumed => InvalidValidationStatus::ConsumedOnL1.into(),
                    _ => unreachable!(),
                }
            } else if record.try_mark_staged(current_staging_epoch_cloned) {
                ValidationStatus::Validated
            } else {
                InvalidValidationStatus::AlreadyIncludedInProposedBlock.into()
            }
        });

        validation_status.unwrap_or(InvalidValidationStatus::NotFound.into())
    }

    pub fn commit_txs(
        &mut self,
        committed_txs: &[TransactionHash],
        rejected_txs: &[TransactionHash],
    ) {
        self.rollback_staging();

        for &tx_hash in committed_txs {
            self.create_record_if_not_exist(tx_hash);
            self.with_record(tx_hash, |r| r.mark_committed()).unwrap();
        }
        for &tx_hash in rejected_txs {
            self.with_record(tx_hash, |r| r.mark_rejected()).expect(
                "Storage inconsistency: a transaction sent to the batcher was removed \
                 unexpectedly.",
            );
        }
    }

    /// Adds a transaction to the transaction manager, return true if the transaction was
    /// successfully added. If the transaction is occupied or already had its hash stored as
    /// committed, it will not be added, and false will be returned.
    // Note: if only the committed hash was known, the transaction will "fill in the blank" in the
    // committed txs storage, to account for commit-before-add tx scenario.
    pub fn add_tx(&mut self, tx: L1HandlerTransaction, block_timestamp: BlockTimestamp) -> bool {
        let tx_hash = tx.tx_hash;
        let is_new_record = self.create_record_if_not_exist(tx_hash);
        self.with_record(tx_hash, move |record| {
            record.tx.set(tx, block_timestamp);
        });

        is_new_record
    }

    pub fn request_cancellation(
        &mut self,
        tx_hash: TransactionHash,
        block_timestamp: BlockTimestamp,
    ) -> Option<BlockTimestamp> {
        self.with_record(tx_hash, |r| r.mark_cancellation_request(block_timestamp)).expect(
            "Should not be possible to request cancellation for non-existent transaction {tx_hash}",
        )
    }

    pub fn is_committed(&self, tx_hash: TransactionHash) -> bool {
        self.records.get(&tx_hash).is_some_and(|record| record.is_committed())
    }

    pub fn exists(&self, tx_hash: TransactionHash) -> bool {
        self.records.contains_key(&tx_hash)
    }

    pub(crate) fn snapshot(&self) -> TransactionManagerSnapshot {
        let mut snapshot = TransactionManagerSnapshot::default();

        for (&tx_hash, record) in self.records.iter() {
            match record.state {
                TransactionState::Rejected => {
                    snapshot.rejected.push(tx_hash);
                    if self.is_staged(tx_hash) {
                        snapshot.rejected_staged.push(tx_hash);
                    }
                }
                TransactionState::Committed => {
                    snapshot.committed.push(tx_hash);
                }
                TransactionState::Pending => {
                    snapshot.uncommitted.push(tx_hash);
                    if self.is_staged(tx_hash) {
                        snapshot.uncommitted_staged.push(tx_hash);
                    }
                }
                TransactionState::CancellationStartedOnL2 => {
                    snapshot.cancellation_started_on_l2.push(tx_hash);
                }
                TransactionState::CancelledOnL2 => {
                    snapshot.cancelled_on_l2.push(tx_hash);
                }
                TransactionState::Consumed => {
                    snapshot.consumed.push(tx_hash);
                }
            }
        }

        snapshot
    }

    fn with_record<F, R>(&mut self, hash: TransactionHash, f: F) -> Option<R>
    where
        F: FnOnce(&mut TransactionRecord) -> R,
    {
        let record = self.records.get_mut_unchecked(hash)?;
        let result = f(record);
        self.maintain_index(hash);
        Some(result)
    }

    fn create_record_if_not_exist(&mut self, hash: TransactionHash) -> bool {
        self.records.insert(hash, TransactionRecord::new(hash.into()))
    }

    fn is_staged(&self, tx_hash: TransactionHash) -> bool {
        self.records
            .get(&tx_hash)
            .is_some_and(|record| record.is_staged(self.current_staging_epoch))
    }

    fn rollback_staging(&mut self) {
        self.current_staging_epoch = self.current_staging_epoch.increment();
    }

    fn maintain_index(&mut self, hash: TransactionHash) {
        if let Some(record) = self.records.get(&hash) {
            let TransactionPayload::Full { created_at_block_timestamp: created_at, .. } = record.tx
            else {
                // We haven't scraped this tx yet, so it isn't indexed.
                return;
            };

            let tx_hash = hash;
            if record.is_proposable() {
                // Assumption: txs will only be added to the index once, on arrival, so this
                // preserves arrival order.
                let tx_hashes = self.proposable_index.entry(created_at).or_default();
                if !tx_hashes.contains(&tx_hash) {
                    tx_hashes.push(tx_hash);
                }
            } else {
                // Remove from the vec for this timestamp, and drop the entry if it becomes empty.
                match self.proposable_index.entry(created_at) {
                    Entry::Occupied(mut entry) => {
                        let tx_hashes = entry.get_mut();
                        if let Some(index_in_vec) = tx_hashes.iter().position(|&h| h == tx_hash) {
                            tx_hashes.remove(index_in_vec);
                            if tx_hashes.is_empty() {
                                entry.remove();
                            }
                        }
                    }
                    Entry::Vacant(_) => {}
                }
            }
        }
    }

    #[cfg(any(feature = "testing", test))]
    pub fn create_for_testing(
        records: Records,
        proposable_index: BTreeMap<BlockTimestamp, Vec<TransactionHash>>,
        current_epoch: StagingEpoch,
        config: TransactionManagerConfig,
        consumed_queue: BTreeMap<BlockTimestamp, Vec<TransactionHash>>,
    ) -> Self {
        Self {
            records,
            proposable_index,
            current_staging_epoch: current_epoch,
            config,
            consumed_queue,
        }
    }
}

impl Default for TransactionManager {
    // Note that new will init the epoch at 1, not 0, this is because a 0 epoch in the transaction
    // manager will make new transactions automatically staged by default in the first block.
    fn default() -> Self {
        Self::new(Duration::from_secs(0), Duration::from_secs(0), Duration::from_secs(0))
    }
}
#[derive(Debug, Default)]
pub(crate) struct TransactionManagerSnapshot {
    pub uncommitted: Vec<TransactionHash>,
    pub uncommitted_staged: Vec<TransactionHash>,
    pub rejected: Vec<TransactionHash>,
    pub rejected_staged: Vec<TransactionHash>,
    pub committed: Vec<TransactionHash>,
    pub cancellation_started_on_l2: Vec<TransactionHash>,
    // NOTE: transition from cancellation-started into cancelled state is done LAZILY only when
    // validation requests are processed against a record.
    pub cancelled_on_l2: Vec<TransactionHash>,
    // NOTE: consumed transactions are removed from the transaction manager LAZILY only when the
    // next consume_tx request is processed (and the timelock has passed).
    pub consumed: Vec<TransactionHash>,
}

#[cfg(any(test, feature = "testing"))]
impl TransactionManagerSnapshot {
    pub fn is_empty(&self) -> bool {
        self.uncommitted.is_empty()
            && self.uncommitted_staged.is_empty()
            && self.rejected.is_empty()
            && self.rejected_staged.is_empty()
            && self.committed.is_empty()
            && self.cancellation_started_on_l2.is_empty()
            && self.cancelled_on_l2.is_empty()
            && self.consumed.is_empty()
    }
}

// Invariant: Monotone-increasing.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct StagingEpoch(u128);

impl StagingEpoch {
    /// Note: initialized to 1, since new l1 handler transactions are initialized with epoch 0 ---
    /// this ensures all new transactions are stageable.
    pub fn new() -> Self {
        Self(1)
    }

    pub fn increment(&mut self) -> Self {
        Self(self.0 + 1)
    }

    pub fn decrement(&mut self) -> Self {
        Self(self.0 - 1)
    }
}

impl Deref for StagingEpoch {
    type Target = u128;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<u128> for StagingEpoch {
    fn from(value: u128) -> Self {
        Self(value)
    }
}

impl Sub<u128> for StagingEpoch {
    type Output = StagingEpoch;

    fn sub(self, rhs: u128) -> Self::Output {
        Self(self.0 - rhs)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransactionManagerConfig {
    // How long to wait before allowing new L1 handler transactions to be proposed (validation is
    // available immediately).
    pub new_l1_handler_tx_cooldown_secs: Duration,
    /// How long to allow a transaction requested for cancellation to be validated against
    /// (proposals are banned upon receiving a cancellation request).
    pub l1_handler_cancellation_timelock_seconds: Duration,
    /// How long to wait before allowing a transaction that was consumed on L1 to be removed from
    /// the transaction managers records.
    // The motivation behind this timelock is to make debugging easier and to be more careful
    // about permanently deleting information.
    // This only delays a cleanup action, so the duration of the timelock wouldn't affect the UX.
    pub l1_handler_consumption_timelock_seconds: Duration,
}
