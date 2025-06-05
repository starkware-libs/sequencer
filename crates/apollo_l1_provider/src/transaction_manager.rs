use apollo_l1_provider_types::{InvalidValidationStatus, ValidationStatus};
use indexmap::{IndexMap, IndexSet};
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::transaction::TransactionHash;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransactionManager {
    /// Storage of all l1 handler transactions --- keeps transactions until they can be safely
    /// removed, like when they are consumed on L1, or fully cancelled on L1.
    pub records: IndexMap<TransactionHash, TransactionRecord>,
    /// Invariant: contains all hashes of transactions that are proposable, and only them.
    /// Structure: [staged_tx1, staged_tx2, ..., staged_txN, unstaged_tx1, unstaged_tx2, ...]
    proposable_index: IndexSet<TransactionHash>,
    /// A transaction record is staged if and only if its epoch equals the current epoch, which is
    /// bumped every block start.
    /// Invariant: Monotone-increasing.
    current_staging_epoch: u128,
}

impl TransactionManager {
    pub fn start_block(&mut self) {
        self.rollback_staging();
    }

    pub fn get_txs(&mut self, n_txs: usize) -> Vec<L1HandlerTransaction> {
        let first_unstaged_index =
            self.proposable_index.partition_point(|&tx_hash| self.is_staged(tx_hash));

        let unstaged_tx_hashes: Vec<_> =
            self.proposable_index[first_unstaged_index..].iter().copied().take(n_txs).collect();

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
        let Some(record) = self.records.get_mut(&tx_hash) else {
            return ValidationStatus::Invalid(InvalidValidationStatus::ConsumedOnL1OrUnknown);
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

        if record.try_mark_staged(self.current_staging_epoch) {
            ValidationStatus::Validated
        } else {
            ValidationStatus::Invalid(InvalidValidationStatus::AlreadyIncludedInProposedBlock)
        }
    }

    pub fn commit_txs(
        &mut self,
        committed_txs: &[TransactionHash],
        rejected_txs: &[TransactionHash],
    ) {
        self.rollback_staging();

        for &tx_hash in committed_txs {
            self.ensure_record(tx_hash);
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
    pub fn add_tx(&mut self, tx: L1HandlerTransaction) -> bool {
        let tx_hash = tx.tx_hash;
        if let Some(entry) = self.records.get_mut(&tx_hash) {
            entry.tx.set(tx);
            return false;
        }

        self.records.insert(
            tx_hash,
            TransactionRecord {
                staged_epoch: self.current_staging_epoch - 1,
                ..TransactionRecord::from(tx)
            },
        );

        let is_new_entry = self.proposable_index.insert(tx_hash);
        assert!(
            is_new_entry,
            "Inconsistent state: new transaction with hash {tx_hash} wasn't in storage but was \
             indexed."
        );

        true
    }

    pub fn is_committed(&self, tx_hash: TransactionHash) -> bool {
        self.records.get(&tx_hash).is_some_and(|record| record.is_committed())
    }

    pub(crate) fn snapshot(&self) -> TransactionManagerSnapshot {
        let mut snapshot = TransactionManagerSnapshot::default();

        for (&tx_hash, record) in &self.records {
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

    fn with_record<F, R>(&mut self, hash: TransactionHash, mut f: F) -> Option<R>
    where
        F: FnMut(&mut TransactionRecord) -> R,
    {
        let record = self.records.get_mut(&hash)?;
        let result = f(record);
        self.maintain_index(hash);
        Some(result)
    }

    fn ensure_record(&mut self, hash: TransactionHash) {
        self.records.entry(hash).or_insert_with(|| TransactionRecord {
            staged_epoch: self.current_staging_epoch - 1,
            ..TransactionRecord::default()
        });
    }

    fn is_staged(&self, tx_hash: TransactionHash) -> bool {
        self.records
            .get(&tx_hash)
            .is_some_and(|record| record.is_staged(self.current_staging_epoch))
    }

    fn rollback_staging(&mut self) {
        self.current_staging_epoch += 1;
    }

    fn maintain_index(&mut self, hash: TransactionHash) {
        if let Some(rec) = self.records.get(&hash) {
            if rec.is_proposable() {
                self.proposable_index.insert(hash);
            } else {
                self.proposable_index.shift_remove(&hash);
            }
        }
    }

    #[cfg(any(feature = "testing", test))]
    pub fn create_for_testing(
        records: IndexMap<TransactionHash, TransactionRecord>,
        proposable_index: IndexSet<TransactionHash>,
        current_epoch: u128,
    ) -> Self {
        Self { records, proposable_index, current_staging_epoch: current_epoch }
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
/// on it ("Domain Entity"). Uses lifecycle metadata to maintain the state of the transaction.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransactionRecord {
    pub tx: TransactionPayload,

    /// State: represents the transaction's state in its lifecycle.
    state: TransactionState,

    /// Metadata fields: use for validity checks in state transitions.
    committed: bool,
    rejected: bool,
    /// A record is staged iff its epoch equals the record owner's (tx manager) epoch counter.
    staged_epoch: u128,
}

impl TransactionRecord {
    pub fn get_unchecked(&self) -> &L1HandlerTransaction {
        match &self.tx {
            TransactionPayload::Full(tx) => tx,
            TransactionPayload::HashOnly(tx_hash) => {
                panic!("Attempted to access transaction payload that is only a hash {tx_hash}.");
            }
        }
    }

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

    pub fn try_mark_staged(&mut self, epoch: u128) -> bool {
        // Sanity check.
        assert!(self.staged_epoch <= epoch, "Epoch counters should not be decreased.");

        let was_unstaged = self.staged_epoch != epoch;
        self.staged_epoch = epoch;
        was_unstaged
    }

    pub fn is_proposable(&self) -> bool {
        matches!(self.state, TransactionState::Pending)
    }

    pub fn is_committed(&self) -> bool {
        matches!(self.state, TransactionState::Committed)
    }

    pub fn is_validatable(&self) -> bool {
        !self.is_committed()
    }

    pub fn is_staged(&self, epoch: u128) -> bool {
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
    Pending,
    Rejected,
}
