use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};

use starknet_api::core::{ContractAddress, Nonce};

use crate::mempool::TransactionReference;

// Note: the derived comparison functionality considers the order guaranteed by the data structures
// used.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct TransactionQueue {
    // Priority queue of transactions with associated priority.
    priority_queue: BTreeSet<PriorityTransaction>,
    // Set of account addresses for efficient existence checks.
    address_to_tx: HashMap<ContractAddress, TransactionReference>,
}

impl TransactionQueue {
    /// Adds a transaction to the mempool, ensuring unique keys.
    /// Panics: if given a duplicate tx.
    // TODO(Mohammad): Add test for two transactions from the same address, expecting specific
    // assert.
    pub fn insert(&mut self, tx_reference: TransactionReference) {
        assert_eq!(
            self.address_to_tx.insert(tx_reference.sender_address, tx_reference.clone()),
            None,
            "Only a single transaction from the same contract class can be in the mempool at a \
             time."
        );
        assert!(
            self.priority_queue.insert(tx_reference.into()),
            "Keys should be unique; duplicates are checked prior."
        );
    }

    // TODO(gilad): remove collect
    pub fn pop_chunk(&mut self, n_txs: usize) -> Vec<TransactionReference> {
        let txs: Vec<TransactionReference> =
            (0..n_txs).filter_map(|_| self.priority_queue.pop_last().map(|tx| tx.0)).collect();
        for tx in &txs {
            self.address_to_tx.remove(&tx.sender_address);
        }

        txs
    }

    /// Returns an iterator of the current eligible transactions for sequencing, ordered by their
    /// priority.
    pub fn iter(&self) -> impl Iterator<Item = &TransactionReference> {
        self.priority_queue.iter().rev().map(|tx| &tx.0)
    }

    pub fn get_nonce(&self, address: ContractAddress) -> Option<Nonce> {
        self.address_to_tx.get(&address).map(|tx| tx.nonce)
    }

    /// Removes the transaction of the given account address from the queue.
    /// This is well-defined, since there is at most one transaction per address in the queue.
    pub fn remove(&mut self, address: ContractAddress) -> bool {
        if let Some(tx) = self.address_to_tx.remove(&address) {
            return self.priority_queue.remove(&tx.into());
        }
        false
    }

    pub fn is_empty(&self) -> bool {
        self.priority_queue.is_empty()
    }
}

/// Encapsulates a transaction reference to assess its order (i.e., gas price).
#[derive(Clone, Debug, derive_more::Deref, derive_more::From)]
struct PendingTransaction(pub TransactionReference);

/// Compare transactions based only on their gas price, using the Eq trait. It ensures that
/// two gas price are either exactly equal or not.
impl PartialEq for PendingTransaction {
    fn eq(&self, other: &PendingTransaction) -> bool {
        self.get_l2_gas_price() == other.get_l2_gas_price() && self.tx_hash == other.tx_hash
    }
}

/// Marks this struct as capable of strict equality comparisons, signaling to the compiler it
/// adheres to equality semantics.
// Note: this depends on the implementation of `PartialEq`, see its docstring.
impl Eq for PendingTransaction {}

impl Ord for PendingTransaction {
    fn cmp(&self, other: &Self) -> Ordering {
        self.get_l2_gas_price()
            .cmp(&other.get_l2_gas_price())
            .then_with(|| self.tx_hash.cmp(&other.tx_hash))
    }
}

impl PartialOrd for PendingTransaction {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// This struct behaves similarly to `PendingTransaction`, encapsulating a transaction reference
/// to assess its order (i.e., tip).
///
/// # See also `PendingTransaction` docstring for details.
#[derive(Clone, Debug, derive_more::Deref, derive_more::From)]
struct PriorityTransaction(pub TransactionReference);

impl PartialEq for PriorityTransaction {
    fn eq(&self, other: &PriorityTransaction) -> bool {
        self.tip == other.tip && self.tx_hash == other.tx_hash
    }
}

impl Eq for PriorityTransaction {}

impl Ord for PriorityTransaction {
    fn cmp(&self, other: &Self) -> Ordering {
        self.tip.cmp(&other.tip).then_with(|| self.tx_hash.cmp(&other.tx_hash))
    }
}

impl PartialOrd for PriorityTransaction {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
