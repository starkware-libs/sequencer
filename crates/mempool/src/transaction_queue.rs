use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};

use starknet_api::block::GasPrice;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::execution_resources::GasAmount;
use starknet_api::transaction::fields::{
    AllResourceBounds,
    ResourceBounds,
    Tip,
    TransactionHash,
    ValidResourceBounds,
};

use crate::mempool::TransactionReference;

#[cfg(test)]
#[path = "transaction_queue_test_utils.rs"]
pub mod transaction_queue_test_utils;

// A queue holding the transaction that with nonces that match account nonces.
// Note: the derived comparison functionality considers the order guaranteed by the data structures
// used.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct TransactionQueue {
    gas_price_threshold: GasPrice,
    // Transactions with gas price above gas price threshold (sorted by tip).
    priority_queue: BTreeSet<PriorityTransaction>,
    // Transactions with gas price below gas price threshold (sorted by price).
    pending_queue: BTreeSet<PendingTransaction>,
    // Set of account addresses for efficient existence checks.
    address_to_tx: HashMap<ContractAddress, TransactionReference>,
}

impl TransactionQueue {
    /// Adds a transaction to the mempool, ensuring unique keys.
    /// Panics: if given a duplicate tx.
    pub fn insert(&mut self, tx_reference: TransactionReference) {
        assert_eq!(
            self.address_to_tx.insert(tx_reference.sender_address, tx_reference),
            None,
            "Only a single transaction from the same contract class can be in the mempool at a \
             time."
        );

        let new_tx_successfully_inserted =
            if tx_reference.get_l2_gas_price() < self.gas_price_threshold {
                self.pending_queue.insert(tx_reference.into())
            } else {
                self.priority_queue.insert(tx_reference.into())
            };
        assert!(
            new_tx_successfully_inserted,
            "Keys should be unique; duplicates are checked prior."
        );
    }

    // TODO(gilad): remove collect
    pub fn pop_ready_chunk(&mut self, n_txs: usize) -> Vec<TransactionReference> {
        let txs: Vec<TransactionReference> =
            (0..n_txs).filter_map(|_| self.priority_queue.pop_last().map(|tx| tx.0)).collect();
        for tx in &txs {
            self.address_to_tx.remove(&tx.sender_address);
        }

        txs
    }

    /// Returns an iterator of the current eligible transactions for sequencing, ordered by their
    /// priority.
    pub fn iter_over_ready_txs(&self) -> impl Iterator<Item = &TransactionReference> {
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

        self.priority_queue.remove(&tx_reference.into())
            || self.pending_queue.remove(&tx_reference.into())
    }

    pub fn has_ready_txs(&self) -> bool {
        self.priority_queue.is_empty()
    }

    pub fn _update_gas_price_threshold(&mut self, threshold: GasPrice) {
        match threshold.cmp(&self.gas_price_threshold) {
            Ordering::Less => self._promote_txs_to_priority(threshold),
            Ordering::Greater => self._demote_txs_to_pending(threshold),
            Ordering::Equal => {}
        }

        self.gas_price_threshold = threshold;
    }

    fn _promote_txs_to_priority(&mut self, threshold: GasPrice) {
        let tmp_split_tx = PendingTransaction(TransactionReference {
            resource_bounds: ValidResourceBounds::AllResources(AllResourceBounds {
                l2_gas: ResourceBounds { max_amount: GasAmount(0), max_price_per_unit: threshold },
                ..Default::default()
            }),
            sender_address: ContractAddress::default(),
            nonce: Nonce::default(),
            tx_hash: TransactionHash::default(),
            tip: Tip::default(),
        });

        // Split off the pending queue at the given transaction higher than the threshold.
        let txs_over_threshold = self.pending_queue.split_off(&tmp_split_tx).into_iter();

        // Insert all transactions from the split point into the priority queue, skip
        // `tmp_split_tx`.
        // Note: extend will reorder transactions by `Tip` during insertion, despite them being
        // initially ordered by fee.
        self.priority_queue.extend(txs_over_threshold.map(|tx| PriorityTransaction::from(tx.0)));
    }

    fn _demote_txs_to_pending(&mut self, threshold: GasPrice) {
        let mut txs_to_remove = Vec::new();

        // Remove all transactions from the priority queue that are below the threshold.
        for priority_tx in &self.priority_queue {
            if priority_tx.get_l2_gas_price() < threshold {
                txs_to_remove.push(*priority_tx);
            }
        }

        for tx in &txs_to_remove {
            self.priority_queue.remove(tx);
        }
        self.pending_queue.extend(txs_to_remove.iter().map(|tx| PendingTransaction::from(tx.0)));
    }
}

/// Encapsulates a transaction reference to assess its order (i.e., gas price).
#[derive(Clone, Copy, Debug, derive_more::Deref, derive_more::From)]
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
#[derive(Clone, Copy, Debug, derive_more::Deref, derive_more::From)]
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
