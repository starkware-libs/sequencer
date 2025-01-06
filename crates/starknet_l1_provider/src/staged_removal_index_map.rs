use std::collections::HashSet;

use indexmap::map::Entry;
use indexmap::IndexMap;
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::transaction::TransactionHash;

/// A TransactionalSet that stores TxEntry objects keyed by their TransactionHash.
/// Each TxEntry knows whether it is staged or unstaged. A separate HashSet tracks which
/// transaction hashes are currently staged, for quick checks.
#[derive(Debug, Default)]
pub struct StagedRemovalIndexMap {
    txs: IndexMap<TransactionHash, TransactionEntry>,
    staged_txs: HashSet<TransactionHash>,
}

impl StagedRemovalIndexMap {
    pub fn new(txs: Vec<L1HandlerTransaction>) -> Self {
        let txs = txs.into_iter().map(|tx| (tx.tx_hash, TransactionEntry::new(tx))).collect();
        Self { txs, ..Default::default() }
    }

    /// Inserts a new TxEntry. If a matching hash already exists, returns the existing TxEntry's
    /// transaction. Otherwise, returns None.
    pub fn insert(&mut self, tx: L1HandlerTransaction) -> Option<L1HandlerTransaction> {
        match self.txs.entry(tx.tx_hash) {
            Entry::Occupied(entry) => Some(entry.get().transaction.clone()),
            Entry::Vacant(entry) => {
                entry.insert(TransactionEntry::new(tx));
                None
            }
        }
    }

    /// Stages up to `n` unstaged transactions in insertion order, returning their transactions.
    /// Each is cloned exactly once. The TxEntry objects remain in `txs`.
    pub fn stage_pop_back(&mut self) -> Option<L1HandlerTransaction> {
        let mut entry = self.txs.values_mut().find(|entry| entry.is_unstaged());
        if let Some(entry) = &mut entry {
            entry.stage();
            let tx_hash = entry.transaction.tx_hash;
            debug_assert_eq!(self.staged_txs.get(&tx_hash), None);
            self.staged_txs.insert(tx_hash);
        }
        entry.cloned().map(|entry| entry.transaction)
    }

    /// Stages a single unstaged transaction by hash, returning true if successfully staged, false
    /// if already staged, and None if unknown.
    pub fn stage_remove(&mut self, tx_hash: TransactionHash) -> Option<bool> {
        let Some(entry) = self.txs.get_mut(&tx_hash) else {
            return None;
        };

        if entry.is_staged() {
            return Some(false);
        }

        debug_assert_eq!(self.staged_txs.get(&tx_hash), None);
        entry.stage();
        self.staged_txs.insert(tx_hash);

        Some(true)
    }

    /// Commits given transactions by removing them entirely and returning the removed transactions.
    /// Uncommitted staged transactions are rolled back to unstaged first.
    ///
    /// Performance note: this method runs in Theta(n), since removing elements from indexmap
    /// requires shifting elements in O(n). Since we are removing multiple elements, recreating
    /// the indexmap is faster than removing each element individually.
    /// This is assumed to be good enough while l1-handler numbers remain low, but if this changes
    /// and we need log(n) removals (amortized), replace indexmap with this (basically a
    /// BTreeIndexMap):
    /// BTreeMap<u32, TransactionEntry>, Hashmap<TransactionHash, u32> and a counter: u32, such that
    /// every new tx is inserted to the map with key counter++ and the counter is not reduced
    /// when removing entries. Once the counter reaches u32::MAX/2 we recreate the DS in Theta(n).
    pub fn commit(&mut self, tx_hashes: &[TransactionHash]) -> Vec<L1HandlerTransaction> {
        self.rollback();
        let tx_hashes: HashSet<_> = tx_hashes.iter().copied().collect();
        if tx_hashes.is_empty() {
            return Vec::new();
        }

        // NOTE: this takes Theta(|self.txs|), see docstring.
        let (committed, not_committed): (Vec<_>, Vec<_>) =
            self.txs.drain(..).partition(|(hash, _)| tx_hashes.contains(hash));
        self.txs.extend(not_committed);

        committed.into_iter().map(|(_, entry)| entry.transaction).collect()
    }

    /// Rolls back all staged transactions, converting them to unstaged.
    pub fn rollback(&mut self) {
        for tx_hash in self.staged_txs.drain() {
            self.txs.entry(tx_hash).and_modify(|entry| entry.unstage());
        }
    }
}

/// Indicates whether a transaction is unstaged or staged.
#[derive(Debug, Clone)]
pub enum TxState {
    Unstaged,
    Staged,
}

/// Wraps an L1HandlerTransaction along with its current TxState,
/// and provides convenience methods for stage/unstage.
#[derive(Debug, Clone)]
pub struct TransactionEntry {
    pub transaction: L1HandlerTransaction,
    pub state: TxState,
}

impl TransactionEntry {
    pub fn new(transaction: L1HandlerTransaction) -> Self {
        Self { transaction, state: TxState::Unstaged }
    }

    pub fn stage(&mut self) {
        if let TxState::Unstaged = self.state {
            self.state = TxState::Staged;
        }
    }

    pub fn is_staged(&self) -> bool {
        matches!(self.state, TxState::Staged)
    }

    pub fn unstage(&mut self) {
        if let TxState::Staged = self.state {
            self.state = TxState::Unstaged;
        }
    }

    pub fn is_unstaged(&self) -> bool {
        matches!(self.state, TxState::Unstaged)
    }
}
