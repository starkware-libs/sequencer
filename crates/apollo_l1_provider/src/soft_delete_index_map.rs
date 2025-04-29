use std::collections::HashSet;

use indexmap::map::Entry;
use indexmap::IndexMap;
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::transaction::TransactionHash;

/// An IndexMap that supports soft deletion of entries.
/// Entries marked as deleted remain hidden in the map, allowing for potential recovery,
/// selective permanent deletion, or rollback before being purged.
// Note: replace with a fully generic struct if there's a need for it.
// Note: replace with a BTreeIndexMap if commit performance becomes an issue, see note in commit.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SoftDeleteIndexMap {
    pub txs: IndexMap<TransactionHash, TransactionEntry>,
    pub staged_txs: HashSet<TransactionHash>,
}

impl SoftDeleteIndexMap {
    pub fn _new() -> Self {
        Self::default()
    }

    /// Inserts a transaction into the map, returning true if the transaction inserted.
    pub fn insert(&mut self, tx: L1HandlerTransaction) -> bool {
        let tx_hash = tx.tx_hash;
        match self.txs.entry(tx_hash) {
            Entry::Occupied(entry) => {
                assert_eq!(entry.get().transaction, tx);
                false
            }
            Entry::Vacant(entry) => {
                entry.insert(TransactionEntry::new(tx));
                true
            }
        }
    }

    /// Soft delete and return a reference to the first unstaged transaction, by insertion order.
    pub fn soft_pop_front(&mut self) -> Option<&L1HandlerTransaction> {
        let entry = self.txs.iter().find(|(_, tx)| tx.is_available());
        let (&tx_hash, _) = entry?;
        self.soft_remove(tx_hash)
    }

    /// Stages the given transaction with the given hash if it exists and is not already staged, and
    /// returns a reference to it.
    pub fn soft_remove(&mut self, tx_hash: TransactionHash) -> Option<&L1HandlerTransaction> {
        let entry = self.txs.get_mut(&tx_hash)?;

        if !entry.is_available() {
            return None;
        }

        assert_eq!(self.staged_txs.get(&tx_hash), None);
        entry.set_state(TxState::Staged);
        self.staged_txs.insert(tx_hash);

        Some(&entry.transaction)
    }

    pub fn get_transaction(&self, tx_hash: &TransactionHash) -> Option<&L1HandlerTransaction> {
        self.txs.get(tx_hash).map(|entry| &entry.transaction)
    }

    /// Rolls back all staged transactions, converting them to unstaged.
    pub fn rollback_staging(&mut self) {
        for tx_hash in self.staged_txs.drain() {
            self.txs.entry(tx_hash).and_modify(|entry| entry.set_state(TxState::Unstaged));
        }
    }

    pub fn is_staged(&self, tx_hash: &TransactionHash) -> bool {
        self.staged_txs.contains(tx_hash)
    }
}

impl From<Vec<L1HandlerTransaction>> for SoftDeleteIndexMap {
    fn from(txs: Vec<L1HandlerTransaction>) -> Self {
        let txs = txs.into_iter().map(|tx| (tx.tx_hash, TransactionEntry::new(tx))).collect();
        SoftDeleteIndexMap { txs, ..Default::default() }
    }
}

/// Indicates whether a transaction is unstaged or staged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TxState {
    Unstaged,
    Staged,
}

/// Wraps an L1HandlerTransaction along with its current TxState,
/// and provides convenience methods for stage/unstage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionEntry {
    pub transaction: L1HandlerTransaction,
    pub state: TxState,
}

impl TransactionEntry {
    pub fn new(transaction: L1HandlerTransaction) -> Self {
        Self { transaction, state: TxState::Unstaged }
    }

    pub fn set_state(&mut self, state: TxState) {
        self.state = state
    }

    pub fn is_available(&self) -> bool {
        match self.state {
            TxState::Unstaged => true,
            TxState::Staged => false,
        }
    }
}
