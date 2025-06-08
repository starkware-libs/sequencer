use std::ops::Deref;

use indexmap::IndexMap;
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::transaction::TransactionHash;

use crate::transaction_manager::StagingEpoch;

/// An entity that wraps a committed L1 handler transaction and all information and decisions made
/// on it ("Domain Entity"). Uses lifecycle metadata to maintain the state of the transaction.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransactionRecord {
    pub tx: TransactionPayload,

    /// State: represents the transaction's state in its lifecycle.
    pub state: TransactionState,

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
    pub fn new(payload: TransactionPayload, staged_epoch: StagingEpoch) -> Self {
        Self { staged_epoch, ..Self::from(payload) }
    }

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

    pub fn is_proposable(&self) -> bool {
        matches!(self.state, TransactionState::Pending)
    }

    pub fn is_committed(&self) -> bool {
        matches!(self.state, TransactionState::Committed)
    }

    /// Answers whether any node can include this transaction in a block. This is generally possible
    /// in all states in its lifecycle, except after it had already been added to block, or (to be
    /// implemented) a short time after it's cancellation was requested on L1.
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
        // Note: this initialized the staged epoch to 0, which is guaranteed to be unstaged since
        // the global epoch is >= 1.
        Self { tx, ..Self::default() }
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

impl From<TransactionHash> for TransactionPayload {
    fn from(hash: TransactionHash) -> Self {
        TransactionPayload::HashOnly(hash)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum TransactionState {
    Committed,
    #[default]
    Pending,
    Rejected,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Records(IndexMap<TransactionHash, TransactionRecord>);

impl Records {
    /// Warning: this is not a safe method to use outside of the transaction managers's
    /// `with_record`.
    pub fn get_mut_unchecked(&mut self, hash: TransactionHash) -> Option<&mut TransactionRecord> {
        self.0.get_mut(&hash)
    }

    pub fn insert(&mut self, hash: TransactionHash, record: TransactionRecord) {
        self.0.insert(hash, record);
    }
}

impl Deref for Records {
    type Target = IndexMap<TransactionHash, TransactionRecord>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<IndexMap<TransactionHash, TransactionRecord>> for Records {
    fn from(map: IndexMap<TransactionHash, TransactionRecord>) -> Self {
        Self(map)
    }
}
