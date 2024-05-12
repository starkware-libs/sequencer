use std::cmp::Ordering;
use std::collections::BTreeSet;

use starknet_mempool_types::mempool_types::ThinTransaction;
// Assumption: for the MVP only one transaction from the same contract class can be in the mempool
// at a time. When this changes, saving the transactions themselves on the queu might no longer be
// appropriate, because we'll also need to stores transactions without indexing them. For example,
// transactions with future nonces will need to be stored, and potentially indexed on block commits.
#[derive(Clone, Debug, Default, derive_more::Deref, derive_more::DerefMut)]
pub struct PriorityQueue(BTreeSet<PQTransaction>);

impl PriorityQueue {
    pub fn push(&mut self, tx: ThinTransaction) {
        let mempool_tx = PQTransaction(tx);
        self.insert(mempool_tx);
    }

    // TODO(gilad): remove collect
    pub fn pop_last_chunk(&mut self, n_txs: usize) -> Vec<ThinTransaction> {
        (0..n_txs).filter_map(|_| self.pop_last().map(|tx| tx.0)).collect()
    }
}

impl FromIterator<ThinTransaction> for PriorityQueue {
    fn from_iter<I: IntoIterator<Item = ThinTransaction>>(iter: I) -> Self {
        PriorityQueue(BTreeSet::from_iter(iter.into_iter().map(PQTransaction)))
    }
}

#[derive(Clone, Debug, derive_more::Deref, derive_more::From)]
pub struct PQTransaction(pub ThinTransaction);

// Compare transactions based on their tip only, which implies `Eq`, because `tip` is uint.
impl PartialEq for PQTransaction {
    fn eq(&self, other: &PQTransaction) -> bool {
        self.tip == other.tip
    }
}

/// Marks PQTransaction as capable of strict equality comparisons, signaling to the compiler it
/// adheres to equality semantics.
// Note: this depends on the implementation of `PartialEq`, see its docstring.
impl Eq for PQTransaction {}

impl Ord for PQTransaction {
    fn cmp(&self, other: &Self) -> Ordering {
        self.tip.cmp(&other.tip)
    }
}

impl PartialOrd for PQTransaction {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
