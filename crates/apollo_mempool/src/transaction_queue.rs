use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap, VecDeque};

use apollo_mempool_types::mempool_types::TransactionQueueSnapshot;
use starknet_api::block::GasPrice;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::transaction::fields::Tip;
use starknet_api::transaction::TransactionHash;

use crate::mempool::TransactionReference;

#[cfg(test)]
#[path = "transaction_queue_test_utils.rs"]
pub mod transaction_queue_test_utils;

// A queue holding the transaction that with nonces that match account nonces.
// Note: the derived comparison functionality considers the order guaranteed by the data structures
// used.
#[derive(Debug, Default)]
pub struct TransactionQueue {
    gas_price_threshold: GasPrice,
    // Transactions with gas price above gas price threshold (FIFO by arrival).
    priority_queue: VecDeque<PriorityTransaction>,
    // Transactions with gas price below gas price threshold (sorted by price).
    pending_queue: BTreeSet<PendingTransaction>,
    // Set of account addresses for efficient existence checks.
    address_to_tx: HashMap<ContractAddress, TransactionReference>,
    // Monotonically increasing counter to preserve arrival order across queues.
    next_arrival_index: u64,
}

impl TransactionQueue {
    /// Adds a transaction to the mempool, ensuring unique keys.
    /// Panics: if given a duplicate tx.
    /// If `validate_resource_bounds` is false, the transaction is added to the priority queue,
    /// regardless of it's L2 gas price bound.
    pub fn insert(&mut self, tx_reference: TransactionReference, validate_resource_bounds: bool) {
        assert_eq!(
            self.address_to_tx.insert(tx_reference.address, tx_reference),
            None,
            "Only a single transaction from the same contract class can be in the mempool at a \
             time."
        );

        let to_pending_queue =
            validate_resource_bounds && tx_reference.max_l2_gas_price < self.gas_price_threshold;
        let arrival_index = {
            let idx = self.next_arrival_index;
            self.next_arrival_index =
                self.next_arrival_index.checked_add(1).expect("arrival index overflow");
            idx
        };

        if to_pending_queue {
            // Store arrival index on pending items so we can preserve FIFO when promoting.
            let inserted =
                self.pending_queue.insert(PendingTransaction::new(tx_reference, arrival_index));
            assert!(inserted, "Keys should be unique; duplicates are checked prior.");
        } else {
            self.priority_queue.push_back(PriorityTransaction::new(tx_reference, arrival_index));
        }
    }

    pub fn priority_queue_len(&self) -> usize {
        self.priority_queue.len()
    }

    pub fn pending_queue_len(&self) -> usize {
        self.pending_queue.len()
    }

    // TODO(gilad): remove collect, if returning an iterator is possible.
    pub fn pop_ready_chunk(&mut self, n_txs: usize) -> Vec<TransactionReference> {
        let mut txs: Vec<TransactionReference> = Vec::with_capacity(n_txs);
        for _ in 0..n_txs {
            if let Some(ptx) = self.priority_queue.pop_front() {
                let tx = ptx.tx;
                self.address_to_tx.remove(&tx.address);
                txs.push(tx);
            } else {
                break;
            }
        }
        txs
    }

    /// Returns an iterator of the current eligible transactions for sequencing, ordered by their
    /// priority.
    pub fn iter_over_ready_txs(&self) -> impl Iterator<Item = &TransactionReference> {
        self.priority_queue.iter().map(|tx| &tx.tx)
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

        // Try remove from priority (VecDeque) by scanning for matching tx hash.
        if let Some(pos) =
            self.priority_queue.iter().position(|ptx| ptx.tx.tx_hash == tx_reference.tx_hash)
        {
            self.priority_queue.remove(pos);
            return true;
        }

        // Try remove from pending (BTreeSet) by finding the exact element (scan then remove).
        if let Some(pending) =
            self.pending_queue.iter().find(|p| p.tx.tx_hash == tx_reference.tx_hash).cloned()
        {
            let removed = self.pending_queue.remove(&pending);
            debug_assert!(removed, "Pending transaction found by scan should be removable.");
            return true;
        }

        false
    }

    /// Removes the given transactions from the queue.
    /// If a transaction is not found, it is ignored.
    pub fn remove_txs(&mut self, txs: &[TransactionReference]) -> Vec<TransactionReference> {
        let mut removed_txs = Vec::new();
        for tx in txs {
            let queued_tx = self.address_to_tx.get(&tx.address);
            if queued_tx.is_some_and(|queued_tx| queued_tx.tx_hash == tx.tx_hash) {
                self.remove(tx.address);
                removed_txs.push(*tx);
            };
        }
        removed_txs
    }

    pub fn has_ready_txs(&self) -> bool {
        !self.priority_queue.is_empty()
    }

    pub fn update_gas_price_threshold(&mut self, threshold: GasPrice) {
        match threshold.cmp(&self.gas_price_threshold) {
            Ordering::Less => self.promote_txs_to_priority(threshold),
            Ordering::Greater => self.demote_txs_to_pending(threshold),
            Ordering::Equal => {}
        }

        self.gas_price_threshold = threshold;
    }

    fn promote_txs_to_priority(&mut self, threshold: GasPrice) {
        let tmp_split_tx = PendingTransaction::split_key(threshold);

        // Split off transactions with price strictly greater than threshold.
        let mut txs_over_threshold: Vec<PendingTransaction> =
            self.pending_queue.split_off(&tmp_split_tx).into_iter().collect();

        // Preserve original arrival order when promoting to priority.
        txs_over_threshold.sort_by_key(|p| p.arrival_index);
        for p in txs_over_threshold {
            self.priority_queue.push_back(PriorityTransaction::new(p.tx, p.arrival_index));
        }
    }

    fn demote_txs_to_pending(&mut self, threshold: GasPrice) {
        // Rebuild priority queue keeping only those >= threshold, demote the rest.
        let mut remaining: VecDeque<PriorityTransaction> =
            VecDeque::with_capacity(self.priority_queue.len());
        for p in self.priority_queue.drain(..) {
            if p.tx.max_l2_gas_price < threshold {
                // Demote to pending; ordering within pending is by price, arrival tracked.
                let _ = self.pending_queue.insert(PendingTransaction::new(p.tx, p.arrival_index));
            } else {
                remaining.push_back(p);
            }
        }
        self.priority_queue = remaining;
    }

    pub fn queue_snapshot(&self) -> TransactionQueueSnapshot {
        let priority_queue = self.priority_queue.iter().map(|tx| tx.tx.tx_hash).collect();
        let pending_queue = self.pending_queue.iter().map(|tx| tx.tx.tx_hash).collect();

        TransactionQueueSnapshot {
            gas_price_threshold: self.gas_price_threshold,
            priority_queue,
            pending_queue,
        }
    }
}

/// Encapsulates a transaction reference and its arrival index for ordering and promotions.
#[derive(Clone, Copy, Debug)]
struct PendingTransaction {
    pub tx: TransactionReference,
    pub arrival_index: u64,
}

impl PendingTransaction {
    fn new(tx: TransactionReference, arrival_index: u64) -> Self {
        PendingTransaction { tx, arrival_index }
    }

    // Helper to construct a split key for BTreeSet::split_off by gas price threshold.
    fn split_key(threshold: GasPrice) -> Self {
        PendingTransaction {
            tx: TransactionReference {
                max_l2_gas_price: threshold,
                address: ContractAddress::default(),
                nonce: Nonce::default(),
                tx_hash: TransactionHash::default(),
                tip: Tip::default(),
            },
            arrival_index: 0,
        }
    }
}

/// Compare transactions based only on their gas price, using the Eq trait. It ensures that
/// two gas price are either exactly equal or not.
impl PartialEq for PendingTransaction {
    fn eq(&self, other: &PendingTransaction) -> bool {
        self.tx.max_l2_gas_price == other.tx.max_l2_gas_price && self.tx.tx_hash == other.tx.tx_hash
    }
}

/// Marks this struct as capable of strict equality comparisons, signaling to the compiler it
/// adheres to equality semantics.
// Note: this depends on the implementation of `PartialEq`, see its docstring.
impl Eq for PendingTransaction {}

impl Ord for PendingTransaction {
    fn cmp(&self, other: &Self) -> Ordering {
        self.tx
            .max_l2_gas_price
            .cmp(&other.tx.max_l2_gas_price)
            .then_with(|| self.arrival_index.cmp(&other.arrival_index))
            .then_with(|| self.tx.tx_hash.cmp(&other.tx.tx_hash))
    }
}

impl PartialOrd for PendingTransaction {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Priority transactions are queued FIFO by arrival.
#[derive(Clone, Copy, Debug)]
struct PriorityTransaction {
    pub tx: TransactionReference,
    pub arrival_index: u64,
}

impl PriorityTransaction {
    fn new(tx: TransactionReference, arrival_index: u64) -> Self {
        PriorityTransaction { tx, arrival_index }
    }
}

impl PartialEq for PriorityTransaction {
    fn eq(&self, other: &PriorityTransaction) -> bool {
        self.tx.tx_hash == other.tx.tx_hash && self.arrival_index == other.arrival_index
    }
}

impl Eq for PriorityTransaction {}

impl Ord for PriorityTransaction {
    fn cmp(&self, other: &Self) -> Ordering {
        self.arrival_index
            .cmp(&other.arrival_index)
            .then_with(|| self.tx.tx_hash.cmp(&other.tx.tx_hash))
    }
}

impl PartialOrd for PriorityTransaction {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
