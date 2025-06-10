use std::collections::BTreeSet;
use std::ops::{Deref, Sub};
use std::time::Duration;

use apollo_l1_provider_types::{InvalidValidationStatus, ValidationStatus};
use starknet_api::block::BlockTimestamp;
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::transaction::TransactionHash;

use crate::transaction_record::{Records, TransactionPayload, TransactionRecord, TransactionState};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransactionManager {
    /// Storage of all l1 handler transactions --- keeps transactions until they can be safely
    /// removed, like when they are consumed on L1, or fully cancelled on L1.
    pub records: Records,
    pub config: TransactionManagerConfig,
    /// Invariant: contains all hashes of transactions that are proposable, and only them.
    /// Structure: [staged_tx1, staged_tx2, ..., staged_txN, unstaged_tx1, unstaged_tx2, ...]
    /// Ordered lexicographically by block timestamp, then tx hash, allowing an efficient
    /// sort-by-time with duplicates (using btreemap with vec<tx_hash> is worse on both
    /// performance and simplicity).
    proposable_index: BTreeSet<ProposableCompositeKey>,
    /// Generation counter used to prevent double usage of an l1 handler transaction in a single
    /// block.
    /// Calling `get_txs` or `validate_tx` tags the touched transactions with the current block
    /// counter, so that further calls will know not to touch them again.
    /// At the start and end (commit) of every block, the counter is incremented, thus "unstaging"
    /// all tagged transactions from the previous block attempt.
    // TODO(Gilad): remove "for rejected" from name when uncommitted is migrated to records DS.
    current_staging_epoch: StagingEpoch,
}

impl TransactionManager {
    pub fn new(new_l1_handler_tx_cooldown_secs: Duration) -> Self {
        Self {
            config: TransactionManagerConfig { new_l1_handler_tx_cooldown_secs },
            records: Default::default(),
            proposable_index: Default::default(),
            current_staging_epoch: StagingEpoch::new(),
        }
    }

    pub fn start_block(&mut self) {
        self.rollback_staging();
    }

    pub fn get_txs(&mut self, n_txs: usize, now: u64) -> Vec<L1HandlerTransaction> {
        let cutoff = now.saturating_sub(self.config.new_l1_handler_tx_cooldown_secs.as_secs());
        let past_cooldown_txs = self.proposable_index.range(
            ..ProposableCompositeKey {
                block_timestamp: cutoff.into(),
                tx_hash: Default::default(),
            },
        );
        // Linear scan, but we expect this to be a small number of transactions (< 10 roughly).
        let unstaged_tx_hashes: Vec<_> = past_cooldown_txs
            .skip_while(|key| self.is_staged(key.tx_hash))
            .map(|key| key.tx_hash)
            .take(n_txs)
            .collect();

        let mut txs = Vec::with_capacity(n_txs);
        let current_staging_epoch = self.current_staging_epoch; // borrow-checker constraint.
        for tx_hash in unstaged_tx_hashes {
            let newly_staged =
                self.with_record(tx_hash, |record| record.try_mark_staged(current_staging_epoch));
            // Sanity check.
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

    pub fn validate_tx(&mut self, tx_hash: TransactionHash) -> ValidationStatus {
        let current_staging_epoch_cloned = self.current_staging_epoch;
        let validation_status = self.with_record(tx_hash, |record| {
            if !record.is_validatable() {
                match record.state {
                    TransactionState::Committed => {
                        InvalidValidationStatus::AlreadyIncludedOnL2.into()
                    }
                    // This will soon also replaced with other states, like `Canceled`, which is
                    // also not-validatable.
                    _ => unreachable!(),
                }
            } else if record.try_mark_staged(current_staging_epoch_cloned) {
                ValidationStatus::Validated
            } else {
                InvalidValidationStatus::AlreadyIncludedInProposedBlock.into()
            }
        });

        validation_status.unwrap_or(InvalidValidationStatus::ConsumedOnL1OrUnknown.into())
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

    pub fn is_committed(&self, tx_hash: TransactionHash) -> bool {
        self.records.get(&tx_hash).is_some_and(|record| record.is_committed())
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
            let TransactionPayload::Full { created_at_block, .. } = &record.tx else {
                return;
            };

            let key = ProposableCompositeKey { block_timestamp: *created_at_block, tx_hash: hash };
            if record.is_proposable() {
                self.proposable_index.insert(key);
            } else {
                self.proposable_index.remove(&key);
            }
        }
    }

    #[cfg(any(feature = "testing", test))]
    pub fn create_for_testing(
        records: Records,
        proposable_index: BTreeSet<ProposableCompositeKey>,
        current_epoch: StagingEpoch,
        config: TransactionManagerConfig,
    ) -> Self {
        Self { records, proposable_index, current_staging_epoch: current_epoch, config }
    }
}

impl Default for TransactionManager {
    // Note that new will init the epoch at 1, not 0, this is because a 0 epoch in the transaction
    // manager will make new transactions automatically staged by default in the first block.
    fn default() -> Self {
        Self::new(Duration::from_secs(0))
    }
}
#[derive(Debug, Default)]
pub(crate) struct TransactionManagerSnapshot {
    pub uncommitted: Vec<TransactionHash>,
    pub uncommitted_staged: Vec<TransactionHash>,
    pub rejected: Vec<TransactionHash>,
    pub rejected_staged: Vec<TransactionHash>,
    pub committed: Vec<TransactionHash>,
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
}

/// Used as a composite key in a set. Note that the ordering of fields significant due to the
/// induced lexicographic ordering from derived `PartialOrd`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProposableCompositeKey {
    /// `PartialOrd` uses this is the primary key for ordering since it appears first in the
    /// struct.
    pub block_timestamp: BlockTimestamp,
    /// `PartialOrd` uses this is the secondary key for sorting since it appears second in the
    /// struct.
    pub tx_hash: TransactionHash,
}
