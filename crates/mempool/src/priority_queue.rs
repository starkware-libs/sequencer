use std::cmp::Ordering;
use std::collections::BTreeSet;

use starknet_mempool_types::mempool_types::ThinTransaction;
// Assumption: for the MVP only one transaction from the same contract class can be in the mempool
// at a time. When this changes, saving the transactions themselves on the queu might no longer be
// appropriate, because we'll also need to stores transactions without indexing them. For example,
// transactions with future nonces will need to be stored, and potentially indexed on block commits.
#[derive(Clone, Debug, Default, derive_more::Deref, derive_more::DerefMut)]
pub struct TransactionPriorityQueue(BTreeSet<PrioritizedTransaction>);

impl TransactionPriorityQueue {
    /// Adds a transaction to the mempool, ensuring unique keys.
    /// Panics: if given a duplicate tx.
    pub fn push(&mut self, tx: ThinTransaction) {
        let mempool_tx = PrioritizedTransaction(tx);
        assert!(self.insert(mempool_tx), "Keys should be unique; duplicates are checked prior.");
    }

    // TODO(gilad): remove collect
    pub fn pop_last_chunk(&mut self, n_txs: usize) -> Vec<ThinTransaction> {
        (0..n_txs).filter_map(|_| self.pop_last().map(|tx| tx.0)).collect()
    }
}

impl From<Vec<ThinTransaction>> for TransactionPriorityQueue {
    fn from(transactions: Vec<ThinTransaction>) -> Self {
        TransactionPriorityQueue(BTreeSet::from_iter(
            transactions.into_iter().map(PrioritizedTransaction),
        ))
    }
}

#[derive(Clone, Debug, derive_more::Deref, derive_more::From)]
pub struct PrioritizedTransaction(pub ThinTransaction);

/// Compare transactions based only on their tip, a uint, using the Eq trait. It ensures that two
/// tips are either exactly equal or not.
impl PartialEq for PrioritizedTransaction {
    fn eq(&self, other: &PrioritizedTransaction) -> bool {
        self.tip == other.tip && self.tx_hash == other.tx_hash
    }
}

/// Marks this struct as capable of strict equality comparisons, signaling to the compiler it
/// adheres to equality semantics.
// Note: this depends on the implementation of `PartialEq`, see its docstring.
impl Eq for PrioritizedTransaction {}

impl Ord for PrioritizedTransaction {
    fn cmp(&self, other: &Self) -> Ordering {
        self.tip.cmp(&other.tip).then_with(|| self.tx_hash.cmp(&other.tx_hash))
    }
}

impl PartialOrd for PrioritizedTransaction {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// TODO: remove when is used.
#[allow(dead_code)]
// Assumption: there are no gaps, and the transactions are received in order.
pub struct AddressPriorityQueue(pub Vec<ThinTransaction>);

// TODO: remove when is used.
#[allow(dead_code)]
impl AddressPriorityQueue {
    pub fn push(&mut self, tx: ThinTransaction) {
        self.0.push(tx);
    }

    pub fn top(&self) -> Option<&ThinTransaction> {
        self.0.first()
    }

    pub fn pop_front(&mut self) -> ThinTransaction {
        self.0.remove(0)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn contains(&self, tx: &ThinTransaction) -> bool {
        self.0.contains(tx)
    }
}
