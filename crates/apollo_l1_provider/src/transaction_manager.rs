use apollo_l1_provider_types::{InvalidValidationStatus, ValidationStatus};
use indexmap::IndexMap;
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::transaction::TransactionHash;

use crate::soft_delete_index_map::SoftDeleteIndexMap;

// TODO(Gilad): migrate uncommitted and rejected storages from the soft delete indexmap into the
// single indexmap that currently holds the committed transactions as records. See the docstring of
// transaction record for how that will work. This change will be implemented in the next few
// commits.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransactionManager {
    uncommitted: SoftDeleteIndexMap,
    rejected: SoftDeleteIndexMap,

    // TODO(Gilad): will soon swallow uncommitted and rejected pools and renamed into `records`.
    committed_records: IndexMap<TransactionHash, TransactionRecord>,
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
        if self.is_committed(tx_hash) {
            return ValidationStatus::Invalid(InvalidValidationStatus::AlreadyIncludedOnL2);
        }

        if self.uncommitted.soft_remove(tx_hash).is_some()
            || self.rejected.soft_remove(tx_hash).is_some()
        {
            ValidationStatus::Validated
        } else if self.uncommitted.is_staged(&tx_hash) || self.rejected.is_staged(&tx_hash) {
            ValidationStatus::Invalid(InvalidValidationStatus::AlreadyIncludedInProposedBlock)
        } else {
            ValidationStatus::Invalid(InvalidValidationStatus::ConsumedOnL1OrUnknown)
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
        let mut rejected = IndexMap::new();
        let mut committed: IndexMap<_, _> = committed_txs
            .iter()
            .copied()
            .map(|tx_hash| (tx_hash, TransactionPayload::HashOnly))
            .collect();

        // Iterate over the uncommitted transactions and check if they are committed or rejected.
        for (hash, entry) in self.uncommitted.txs.drain(..) {
            // Each rejected transaction is added to the rejected pool.
            if rejected_txs.contains(&hash) {
                rejected.insert(hash, entry);
            } else if committed.contains_key(&hash) {
                committed.get_mut(&hash).unwrap().set(entry.tx);
            } else {
                // If a transaction is not committed or rejected, it is added back to the
                // uncommitted pool.
                uncommitted.insert(hash, entry);
            }
        }

        self.rejected.txs.extend(rejected);

        // Assign the remaining uncommitted txs to the uncommitted pool, which was was drained.
        self.uncommitted.txs = uncommitted;

        // Add all committed tx hashes to the committed buffer, regardless of if they're known or
        // not, in case we haven't scraped them yet and another node did.
        for (tx_hash, payload) in committed {
            self.committed_records
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
        if let Some(entry) = self.committed_records.get_mut(&tx.tx_hash) {
            entry.tx.set(tx);
            return false;
        }

        if self.rejected.txs.contains_key(&tx.tx_hash) {
            return false;
        }

        self.uncommitted.insert(tx)
    }

    pub fn is_committed(&self, tx_hash: TransactionHash) -> bool {
        self.committed_records.contains_key(&tx_hash)
    }

    pub(crate) fn snapshot(&self) -> TransactionManagerSnapshot {
        TransactionManagerSnapshot {
            uncommitted: self.uncommitted.txs.keys().copied().collect(),
            uncommitted_staged: self.uncommitted.staged_txs.iter().copied().collect(),
            rejected: self.rejected.txs.keys().copied().collect(),
            rejected_staged: self.rejected.staged_txs.iter().copied().collect(),
            committed: self.committed_records.keys().copied().collect(),
        }
    }

    fn rollback_staging(&mut self) {
        self.uncommitted.rollback_staging();
        self.rejected.rollback_staging();
    }

    #[cfg(any(feature = "testing", test))]
    pub fn create_for_testing(
        uncommitted: SoftDeleteIndexMap,
        rejected: SoftDeleteIndexMap,
        committed: IndexMap<TransactionHash, TransactionRecord>,
    ) -> Self {
        Self { uncommitted, rejected, committed_records: committed }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum TransactionPayload {
    #[default]
    HashOnly,
    Full(L1HandlerTransaction),
}

impl TransactionPayload {
    pub fn set(&mut self, tx: L1HandlerTransaction) {
        *self = tx.into();
    }
}

impl From<L1HandlerTransaction> for TransactionPayload {
    fn from(tx: L1HandlerTransaction) -> Self {
        TransactionPayload::Full(tx)
    }
}

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
    current_state: TransactionState,
    committed: bool,
}

impl TransactionRecord {
    pub fn mark_committed(&mut self) {
        self.current_state = TransactionState::Committed;
        self.committed = true;
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
}
