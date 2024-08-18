use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};

use starknet_api::core::{ContractAddress, Nonce};

use crate::mempool::TransactionReference;

// A queue holding the transaction that with nonces that match account nonces.
// Note: the derived comparison functionality considers the order guaranteed by the data structures
// used.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct TransactionQueue {
    // Transactions with gas price above base price (sorted by tip).
    priority_queue: BTreeSet<PriorityTransaction>,
    // Transactions with gas price below base price (sorted by price).
    pending_queue: BTreeSet<PendingTransaction>,
    gas_price_threshold: u128,
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

        if tx_reference.get_l2_gas_price() < self.gas_price_threshold {
            assert!(
                self.pending_queue.insert(tx_reference.into()),
                "Keys should be unique; duplicates are checked prior."
            );
        } else {
            assert!(
                self.priority_queue.insert(tx_reference.into()),
                "Keys should be unique; duplicates are checked prior."
            );
        }
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
        let Some(tx_reference) = self.address_to_tx.remove(&address) else {
            return false;
        };

        self.priority_queue.remove(&tx_reference.clone().into())
            || self.pending_queue.remove(&tx_reference.into())
    }

    pub fn is_empty(&self) -> bool {
        self.priority_queue.is_empty() && self.pending_queue.is_empty()
    }

    pub fn update_gas_price_threshold(&mut self, new_threshold: u128) {
        if new_threshold < self.gas_price_threshold {
            for tx in self.pending_queue.iter() {
                if tx.get_l2_gas_price() >= new_threshold {
                    self.priority_queue.insert(tx.0.clone().into());
                } else {
                    break;
                }
            }
        } else {
            for tx in self.priority_queue.iter() {
                if tx.get_l2_gas_price() < new_threshold {
                    self.pending_queue.insert(tx.0.clone().into());
                }
            }
        }
        self.gas_price_threshold = new_threshold;
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
/// to assess its order (i.e., tip); see its documentation for more details.
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
