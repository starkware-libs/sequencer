use apollo_l1_provider_types::{InvalidValidationStatus, ValidationStatus};
use indexmap::IndexMap;
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::transaction::TransactionHash;

use crate::soft_delete_index_map::SoftDeleteIndexMap;

// TODO(Gilad): migrate uncommitted storage from the soft delete indexmap into the
// single indexmap that currently holds the committed and rejected transactions as records. See the
// docstring of transaction record for how that will work. This change will be implemented in the
// next few commits.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransactionManager {
    uncommitted: SoftDeleteIndexMap,

    // TODO(Gilad): holds committed + rejected and will soon swallow uncommitted pool and renamed
    // into `records`.
    processed_records: IndexMap<TransactionHash, TransactionRecord>,
    /// Generation counter used to prevent double usage of an l1 handler transaction in a single
    /// block.
    /// Calling `get_txs` or `validate_tx` tags the touched transactions with the current block
    /// counter, so that further calls will know not to touch them again.
    /// At the start and end (commit) of every block, the counter is incremented, thus "unstaging"
    /// all tagged transactions from the previous block attempt.
    // TODO(Gilad): remove "for rejected" from name when uncommitted is migrated to records DS.
    current_staging_epoch_for_rejected: StagingEpoch,
}

impl TransactionManager {
    pub fn start_block(&mut self) {
        self.rollback_staging();
    }

    pub fn get_txs(&mut self, n_txs: usize) -> Vec<L1HandlerTransaction> {
        let mut txs = Vec::with_capacity(n_txs);

        for _ in 0..n_txs {
            match self.uncommitted.soft_pop_front().cloned() {
                Some(tx) => txs.push(tx),
                None => break,
            }
        }
        txs
    }

    pub fn validate_tx(&mut self, tx_hash: TransactionHash) -> ValidationStatus {
        let Some(record) = self.processed_records.get_mut(&tx_hash) else {
            // This whole check will soon be removed and replaced with just Invalid(Unknown), once
            // uncommitted are part of the records above.
            return if self.uncommitted.soft_remove(tx_hash).is_some() {
                ValidationStatus::Validated
            } else if self.uncommitted.is_staged(&tx_hash) {
                ValidationStatus::Invalid(InvalidValidationStatus::AlreadyIncludedInProposedBlock)
            } else {
                ValidationStatus::Invalid(InvalidValidationStatus::ConsumedOnL1OrUnknown)
            };
        };

        if !record.is_validatable() {
            match record.state {
                TransactionState::Committed => {
                    return ValidationStatus::Invalid(InvalidValidationStatus::AlreadyIncludedOnL2);
                }
                // This will soon also replaced with other states, like `Canceled`, which is also
                // not-validatable.
                _ => unreachable!(),
            }
        }

        if record.try_mark_staged(self.current_staging_epoch_for_rejected) {
            ValidationStatus::Validated
        } else {
            ValidationStatus::Invalid(InvalidValidationStatus::AlreadyIncludedInProposedBlock)
        }
    }

    /// This function does the following:
    /// 1) Rolls back the uncommitted and rejected staging pools.
    /// 2) Moves all newly committed transactions from the uncommitted pool to the committed pool.
    /// 3) Moves all newly rejected transactions from the uncommitted pool to the rejected pool.
    ///
    /// # Performance
    /// This function has linear complexity in the number of known transactions and the
    /// number of transactions being committed. This is acceptable while the number of
    /// L1 handler transactions remains low. If higher performance becomes necessary (e.g.,
    /// requiring amortized log(n) operations), consider replacing `IndexMap` with a
    /// structure like: `BTreeMap<u32, TransactionEntry>'.
    pub fn commit_txs(
        &mut self,
        committed_txs: &[TransactionHash],
        rejected_txs: &[TransactionHash],
    ) {
        // When committing transactions, we don't need to have staged transactions.
        self.rollback_staging();

        let mut uncommitted = IndexMap::new();

        // Note: the duplication below is temporary and solely due to uncommitted
        // still not be migrated to the records DS, commit_txs will be much simpler once that is
        // done.
        let mut rejected: IndexMap<_, _> = rejected_txs
            .iter()
            .copied()
            .map(|tx_hash| (tx_hash, TransactionPayload::HashOnly(tx_hash)))
            .collect();
        let mut committed: IndexMap<_, _> = committed_txs
            .iter()
            .copied()
            .map(|tx_hash| (tx_hash, TransactionPayload::HashOnly(tx_hash)))
            .collect();

        // Iterate over the uncommitted transactions and check if they are committed or rejected.
        for (hash, entry) in self.uncommitted.txs.drain(..) {
            // Each rejected transaction is added to the rejected pool.
            if rejected_txs.contains(&hash) {
                rejected.get_mut(&hash).unwrap().set(entry.tx);
            } else if committed.contains_key(&hash) {
                committed.get_mut(&hash).unwrap().set(entry.tx);
            } else {
                // If a transaction is not committed or rejected, it is added back to the
                // uncommitted pool.
                uncommitted.insert(hash, entry);
            }
        }

        for (tx_hash, payload) in rejected {
            self.processed_records.entry(tx_hash).or_insert_with(|| payload.into()).mark_rejected();
        }

        // Assign the remaining uncommitted txs to the uncommitted pool, which was was drained.
        self.uncommitted.txs = uncommitted;

        // Add all committed tx hashes to the committed buffer, regardless of if they're known or
        // not, in case we haven't scraped them yet and another node did.
        for (tx_hash, payload) in committed {
            self.processed_records
                .entry(tx_hash)
                .or_insert_with(|| payload.into())
                .mark_committed();
        }
    }

    /// Adds a transaction to the transaction manager, return true if the transaction was
    /// successfully added. If the transaction is occupied or already had its hash stored as
    /// committed, it will not be added, and false will be returned.
    // Note: if only the committed hash was known, the transaction will "fill in the blank" in the
    // committed txs storage, to account for commit-before-add tx scenario.
    pub fn add_tx(&mut self, tx: L1HandlerTransaction) -> bool {
        if let Some(entry) = self.processed_records.get_mut(&tx.tx_hash) {
            entry.tx.set(tx);
            return false;
        }

        self.uncommitted.insert(tx)
    }

    pub fn is_committed(&self, tx_hash: TransactionHash) -> bool {
        self.processed_records.get(&tx_hash).is_some_and(|record| record.is_committed())
    }

    pub(crate) fn snapshot(&self) -> TransactionManagerSnapshot {
        let mut snapshot = TransactionManagerSnapshot {
            uncommitted: self.uncommitted.txs.keys().copied().collect(),
            uncommitted_staged: self.uncommitted.staged_txs.iter().copied().collect(),
            ..TransactionManagerSnapshot::default()
        };

        for (&tx_hash, record) in &self.processed_records {
            match record.state {
                TransactionState::Rejected => {
                    snapshot.rejected.push(tx_hash);
                    if self.is_staged_rejected(tx_hash) {
                        snapshot.rejected_staged.push(tx_hash);
                    }
                }
                TransactionState::Committed => {
                    snapshot.committed.push(tx_hash);
                }
                TransactionState::Pending => todo!("Will replace `uncommitted` buffer soon."),
            }
        }

        snapshot
    }

    // TODO(Gilad): rename into `is_staged` soon, when uncommitted is migrated to records DS.
    fn is_staged_rejected(&self, tx_hash: TransactionHash) -> bool {
        self.processed_records
            .get(&tx_hash)
            .is_some_and(|record| record.is_staged(self.current_staging_epoch_for_rejected))
    }

    fn rollback_staging(&mut self) {
        self.uncommitted.rollback_staging();
        self.current_staging_epoch_for_rejected.increment();
    }

    #[cfg(any(feature = "testing", test))]
    pub fn create_for_testing(
        uncommitted: SoftDeleteIndexMap,
        processed_records: IndexMap<TransactionHash, TransactionRecord>,
    ) -> Self {
        Self {
            uncommitted,
            processed_records,
            current_staging_epoch_for_rejected: StagingEpoch::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransactionPayload {
    HashOnly(TransactionHash),
    Full(L1HandlerTransaction),
}

impl TransactionPayload {
    pub fn set(&mut self, tx: L1HandlerTransaction) {
        *self = tx.into();
    }

    pub fn tx_hash(&self) -> TransactionHash {
        match self {
            TransactionPayload::HashOnly(hash) => *hash,
            TransactionPayload::Full(tx) => tx.tx_hash,
        }
    }
}

impl Default for TransactionPayload {
    fn default() -> Self {
        TransactionPayload::HashOnly(TransactionHash::default())
    }
}

impl From<L1HandlerTransaction> for TransactionPayload {
    fn from(tx: L1HandlerTransaction) -> Self {
        TransactionPayload::Full(tx)
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

/// An entity that wraps a committed L1 handler transaction and all information and decisions made
/// on it ("Domain Entity").
///
/// Future versions will accumulate all lifecycle metadata (timestamps, staging, validation,
/// cancellation, etc.) and will include API for querying the tx about its current state based on
/// all of said metadata.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransactionRecord {
    pub tx: TransactionPayload,

    /// State: represents the transaction's state in its lifecycle.
    state: TransactionState,

    /// Metadata fields: use for validity/sanity checks in state transitions, to catch bugs that
    /// can't be captured by state alone.
    /// In other words, the state is the state machine state, and the metadata fields are used to
    /// calculate whether a given state transition is valid.
    committed: bool,
    rejected: bool,
    /// A record is staged iff its epoch equals the record owner's (tx manager) epoch counter.
    staged_epoch: StagingEpoch,
}

impl TransactionRecord {
    pub fn mark_committed(&mut self) {
        // Can't return error because committing only part of a block leaves the provider in an
        // undetermined state.
        assert!(
            !self.committed,
            "L1 handler transaction {} committed twice, this may lead to l2 reorgs,",
            self.tx.tx_hash()
        );

        self.state = TransactionState::Committed;
        self.committed = true;
    }

    // Note: double reject not currently checked.
    pub fn mark_rejected(&mut self) {
        // Pedantic, this is unlikely to happen.
        assert!(
            !self.committed,
            "Attempted to reject a committed transaction {}",
            self.tx.tx_hash()
        );

        self.state = TransactionState::Rejected;
        self.rejected = true;
    }

    /// Try to stage an l1 handler transaction, which means that we allow to include it in the
    /// current proposed or validated block. If already included in a block, this test will return
    /// false, thus preventing double-inclusion in the block. Staging is reset at the start of every
    /// block to ensure this.
    pub fn try_mark_staged(&mut self, epoch: StagingEpoch) -> bool {
        // Sanity check.
        assert!(self.staged_epoch <= epoch, "Epoch counters should not be decreased.");

        let was_unstaged = !self.is_staged(epoch);
        self.staged_epoch = epoch;
        was_unstaged
    }

    pub fn is_committed(&self) -> bool {
        matches!(self.state, TransactionState::Committed)
    }

    /// Answers whether any node can include this transaction in a block. This is generally possible
    /// in all states in its lifecycle, except after it had already been added to block, or (to be
    /// inmplemented) a short time after it's cancellation was requested on L1.
    /// In particular, this includes states like: a rejected transaction, a new timelocked
    /// transaction (to be implemented), a transaction whose cancellation was requested on L1 too
    /// recently (there will be a timelock for this).
    pub fn is_validatable(&self) -> bool {
        !self.is_committed()
    }

    pub fn is_staged(&self, epoch: StagingEpoch) -> bool {
        self.staged_epoch == epoch
    }
}

impl From<L1HandlerTransaction> for TransactionRecord {
    fn from(tx: L1HandlerTransaction) -> Self {
        TransactionPayload::from(tx).into()
    }
}

impl From<TransactionPayload> for TransactionRecord {
    fn from(tx: TransactionPayload) -> Self {
        Self { tx, ..Self::default() }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum TransactionState {
    Committed,
    #[default]
    Pending, // Currently unused, only useful for Default, will be used soon though.
    Rejected,
}

// Invariant: Monotone-increasing.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct StagingEpoch(u128);

impl StagingEpoch {
    pub fn new() -> Self {
        Self(1)
    }

    pub fn increment(&mut self) {
        self.0 = self.0.checked_add(1).expect("Staging epoch overflow, unlikely.");
    }
}
