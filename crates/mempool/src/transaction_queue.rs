use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};

use starknet_api::core::{ContractAddress, Nonce};

use crate::mempool::TransactionReference;

// Note: the derived comparison functionality considers the order guaranteed by the data structures
// used.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct TransactionQueue {
    // Priority queue of transactions with associated priority.
    queue: BTreeSet<QueuedTransaction>,
    // Set of account addresses for efficient existence checks.
    address_to_tx: HashMap<ContractAddress, TransactionReference>,
}

impl TransactionQueue {
    /// Adds a transaction to the mempool, ensuring unique keys.
    /// Panics: if given a duplicate tx.
    // TODO(Mohammad): Add test for two transactions from the same address, expecting specific
    // assert.
    pub fn insert(&mut self, tx: TransactionReference) {
        assert_eq!(
            self.address_to_tx.insert(tx.sender_address, tx),
            None,
            "Only a single transaction from the same contract class can be in the mempool at a \
             time."
        );
        assert!(
            self.queue.insert(tx.into()),
            "Keys should be unique; duplicates are checked prior."
        );
    }

    // TODO(gilad): remove collect
    pub fn pop_chunk(&mut self, n_txs: usize) -> Vec<TransactionReference> {
        let txs: Vec<TransactionReference> =
            (0..n_txs).filter_map(|_| self.queue.pop_last().map(|tx| tx.0)).collect();
        for tx in &txs {
            self.address_to_tx.remove(&tx.sender_address);
        }

        txs
    }

    /// Returns an iterator of the current eligible transactions for sequencing, ordered by their
    /// priority.
    pub fn iter(&self) -> impl Iterator<Item = &TransactionReference> {
        self.queue.iter().rev().map(|tx| &tx.0)
    }

    pub fn get_nonce(&self, address: ContractAddress) -> Option<Nonce> {
        self.address_to_tx.get(&address).map(|tx| tx.nonce)
    }

    /// Removes the transaction of the given account address from the queue.
    /// This is well-defined, since there is at most one transaction per address in the queue.
    pub fn remove(&mut self, address: ContractAddress) -> bool {
        if let Some(tx) = self.address_to_tx.remove(&address) {
            return self.queue.remove(&tx.into());
        }
        false
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

/// Encapsulates a transaction reference to assess its order (i.e., priority).
#[derive(Clone, Debug, derive_more::Deref, derive_more::From)]
struct QueuedTransaction(pub TransactionReference);

/// Compare transactions based only on their tip, a uint, using the Eq trait. It ensures that two
/// tips are either exactly equal or not.
impl PartialEq for QueuedTransaction {
    fn eq(&self, other: &QueuedTransaction) -> bool {
        self.tip == other.tip && self.tx_hash == other.tx_hash
    }
}

/// Marks this struct as capable of strict equality comparisons, signaling to the compiler it
/// adheres to equality semantics.
// Note: this depends on the implementation of `PartialEq`, see its docstring.
impl Eq for QueuedTransaction {}

impl Ord for QueuedTransaction {
    fn cmp(&self, other: &Self) -> Ordering {
        self.tip.cmp(&other.tip).then_with(|| self.tx_hash.cmp(&other.tx_hash))
    }
}

impl PartialOrd for QueuedTransaction {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
