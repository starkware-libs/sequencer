use std::collections::{HashMap, HashSet, VecDeque};

use apollo_mempool_types::mempool_types::TransactionQueueSnapshot;
use indexmap::IndexSet;
use starknet_api::block::{GasPrice, UnixTimestamp};
use starknet_api::core::ContractAddress;
use starknet_api::transaction::TransactionHash;
use tracing::debug;

use crate::mempool::TransactionReference;
use crate::transaction_queue_trait::{RewindData, TransactionQueueTrait};

/// A FIFO (First-In-First-Out) transaction queue that orders transactions by arrival time.
/// Used in Echonet mode to preserve the original transaction order from the source chain.
#[derive(Debug)]
pub struct FifoTransactionQueue {
    // Queue of transaction hashes ordered by arrival time (FIFO).
    queue: VecDeque<TransactionHash>,
    // Map from transaction hash to transaction reference for efficient lookups.
    hash_to_tx: HashMap<TransactionHash, TransactionReference>,
    // Map from transaction hash to timestamp.
    hash_to_timestamp: HashMap<TransactionHash, UnixTimestamp>,
    // Last timestamp returned by get_timestamp_for_batch() - used to filter transactions in
    // pop_ready_chunk.
    last_returned_timestamp: Option<UnixTimestamp>,
}

impl FifoTransactionQueue {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            hash_to_tx: HashMap::new(),
            hash_to_timestamp: HashMap::new(),
            last_returned_timestamp: None,
        }
    }
}

impl TransactionQueueTrait for FifoTransactionQueue {
    fn insert(&mut self, tx_reference: TransactionReference, _validate_resource_bounds: bool) {
        let tx_hash = tx_reference.tx_hash;

        // Timestamp must be set via update_timestamps before insert
        let timestamp = self
            .hash_to_timestamp
            .get(&tx_hash)
            .expect("FIFO insert: transaction must have timestamp set before insertion");

        // Add transaction to queue in FIFO order
        self.queue.push_back(tx_hash);
        self.hash_to_tx.insert(tx_hash, tx_reference);

        debug!(
            "FIFO insert: tx_hash={}, timestamp={}, queue_len={}",
            tx_hash,
            timestamp,
            self.queue.len()
        );
    }

    fn pop_ready_chunk(&mut self, n_txs: usize) -> Vec<TransactionReference> {
        // If get_ts() hasn't been called, return empty vec
        let Some(timestamp_threshold) = self.last_returned_timestamp else {
            return Vec::new();
        };

        // Collect transactions that match the timestamp threshold
        let mut result = Vec::new();

        for &tx_hash in &self.queue {
            if result.len() >= n_txs {
                break;
            }

            if let Some(&tx_timestamp) = self.hash_to_timestamp.get(&tx_hash) {
                if tx_timestamp == timestamp_threshold {
                    if let Some(tx_ref) = self.hash_to_tx.remove(&tx_hash) {
                        debug!(
                            "FIFO pop_ready_chunk: popping tx_hash={}, timestamp={}, \
                             last_returned_timestamp={:?}",
                            tx_hash, tx_timestamp, self.last_returned_timestamp
                        );
                        result.push(tx_ref);
                        // Keep timestamp in map for potential rewind
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        self.queue.drain(..result.len());

        result
    }

    fn remove_txs(&mut self, txs: &[TransactionReference]) -> Vec<TransactionReference> {
        let mut removed_txs = Vec::new();
        for tx in txs {
            if self.remove_by_hash(tx.tx_hash) {
                removed_txs.push(*tx);
            }
        }
        removed_txs
    }

    fn has_ready_txs(&self) -> bool {
        // If get_timestamp() hasn't been called yet, no txs are ready
        let Some(timestamp_threshold) = self.last_returned_timestamp else {
            return false;
        };

        // Check if the first tx in queue has the same timestamp as last_returned_timestamp
        if let Some(first_hash) = self.queue.front() {
            if let Some(&tx_timestamp) = self.hash_to_timestamp.get(first_hash) {
                return tx_timestamp == timestamp_threshold;
            }
        }

        false
    }

    fn iter_over_ready_txs(&self) -> Box<dyn Iterator<Item = &TransactionReference> + '_> {
        Box::new(self.queue.iter().filter_map(|hash| self.hash_to_tx.get(hash)))
    }

    fn queue_snapshot(&self) -> TransactionQueueSnapshot {
        // FIFO queue doesn't have priority/pending distinction.
        let priority_queue: Vec<TransactionHash> = self.queue.iter().copied().collect();
        TransactionQueueSnapshot {
            gas_price_threshold: GasPrice::default(),
            priority_queue,
            pending_queue: Vec::new(),
        }
    }

    fn rewind_txs(
        &mut self,
        rewind_data: crate::transaction_queue_trait::RewindData<'_>,
    ) -> IndexSet<TransactionHash> {
        // Extract FIFO-specific data
        let RewindData::Fifo { staged_tx_refs, committed_nonces, rejected_tx_hashes } = rewind_data
        else {
            panic!("FifoTransactionQueue received wrong RewindData variant");
        };

        // Group staged transactions by address
        let staged_by_address: HashMap<ContractAddress, Vec<&TransactionReference>> =
            staged_tx_refs.iter().fold(HashMap::new(), |mut acc, tx| {
                acc.entry(tx.address).or_default().push(tx);
                acc
            });

        let mut rewound_hashes = IndexSet::new();
        let mut addresses_to_rewind = HashSet::new();

        // First pass: Determine which addresses should have transactions rewound
        for (address, txs) in &staged_by_address {
            if let Some(&committed_nonce) = committed_nonces.get(address) {
                // Address has committed transactions - check if the following tx is rejected
                let following_tx = txs.iter().find(|tx| tx.nonce == committed_nonce);

                match following_tx {
                    Some(following_tx_ref) => {
                        // Following tx exists - check if it's rejected
                        if rejected_tx_hashes.contains(&following_tx_ref.tx_hash) {
                            // Following tx is rejected - don't rewind any transactions for this
                            // address
                            continue;
                        }
                        // Following tx is not rejected - rewind all non-committed txs
                        addresses_to_rewind.insert(*address);
                    }
                    None => {
                        // Following tx doesn't exist - rewind all non-committed txs
                        addresses_to_rewind.insert(*address);
                    }
                }
            } else {
                // Address has no committed transactions (was staged but NOT committed)
                // Find the first tx (lowest nonce) for this address
                if let Some(first_tx) = txs.iter().min_by_key(|tx| tx.nonce) {
                    // If the first tx is rejected, don't rewind anything
                    if rejected_tx_hashes.contains(&first_tx.tx_hash) {
                        continue;
                    }
                }
                // First tx is not rejected (or doesn't exist) - rewind all transactions
                addresses_to_rewind.insert(*address);
            }
        }

        // Second pass: Rewind transactions from addresses that need rewinding
        // Rewind all non-committed transactions (including rejected ones if address is being
        // rewound)
        // Track which transactions we've already rewound to prevent duplicates
        let mut already_rewound: HashSet<TransactionHash> = HashSet::new();

        for tx_ref in staged_tx_refs.iter().rev() {
            let address_needs_rewind = addresses_to_rewind.contains(&tx_ref.address);
            if !address_needs_rewind {
                continue;
            }

            // Check if this transaction was committed
            let is_committed =
                committed_nonces.get(&tx_ref.address).is_some_and(|&cn| tx_ref.nonce < cn);

            if !is_committed && !already_rewound.contains(&tx_ref.tx_hash) {
                let tx_hash = tx_ref.tx_hash;

                // Timestamp must already exist for rewound transactions
                let timestamp = *self
                    .hash_to_timestamp
                    .get(&tx_hash)
                    .expect("Rewound transaction must have a timestamp already set");

                debug!(
                    "FIFO rewind: tx_hash={}, timestamp={}, queue_before={:?}",
                    tx_hash, timestamp, self.queue
                );

                // Add transaction to FRONT of queue (ensures rewound txs are processed before new
                // txs)
                self.queue.push_front(tx_hash);
                self.hash_to_tx.insert(tx_hash, *tx_ref);

                rewound_hashes.insert(tx_ref.tx_hash);
                already_rewound.insert(tx_ref.tx_hash);
            }
        }

        rewound_hashes
    }

    fn priority_queue_len(&self) -> usize {
        self.queue.len()
    }

    fn pending_queue_len(&self) -> usize {
        0
    }

    fn get_first_queued_tx_timestamp(&self) -> Option<UnixTimestamp> {
        self.queue.front().and_then(|hash| self.hash_to_timestamp.get(hash)).copied()
    }

    fn set_last_returned_timestamp(&mut self, timestamp: UnixTimestamp) {
        self.last_returned_timestamp = Some(timestamp);
    }

    fn get_last_returned_timestamp(&self) -> Option<UnixTimestamp> {
        self.last_returned_timestamp
    }

    fn update_timestamp(&mut self, tx_hash: TransactionHash, timestamp: UnixTimestamp) {
        self.hash_to_timestamp.insert(tx_hash, timestamp);
    }
}

impl FifoTransactionQueue {
    /// Removes the transaction with the given hash from the queue.
    /// Returns true if the transaction was found and removed, false otherwise.
    fn remove_by_hash(&mut self, tx_hash: TransactionHash) -> bool {
        if self.hash_to_tx.remove(&tx_hash).is_some() {
            self.hash_to_timestamp.remove(&tx_hash);
            if let Some(pos) = self.queue.iter().position(|&hash| hash == tx_hash) {
                self.queue.remove(pos);
            }
            true
        } else {
            false
        }
    }
}
