use std::collections::{HashMap, VecDeque};

use apollo_mempool_types::mempool_types::TransactionQueueSnapshot;
use starknet_api::block::{GasPrice, UnixTimestamp};
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::transaction::TransactionHash;
use tracing::{debug, info};

use crate::mempool::TransactionReference;
use crate::transaction_queue_trait::TransactionQueueTrait;

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

        debug!("FIFO insert: tx_hash={}, queue_len_before={}", tx_hash, self.queue.len());

        // Add transaction to queue in FIFO order
        self.queue.push_back(tx_hash);
        self.hash_to_tx.insert(tx_hash, tx_reference);

        // Check if timestamp exists in mapping, otherwise use 0 as fallback
        if let Some(&timestamp) = self.hash_to_timestamp.get(&tx_hash) {
            info!(
                "FIFO insert: tx_hash={}, timestamp={} (stored), queue_len={}",
                tx_hash,
                timestamp,
                self.queue.len()
            );
        } else {
            self.hash_to_timestamp.insert(tx_hash, 0);
            info!(
                "FIFO insert: tx_hash={}, timestamp=0 (fallback), queue_len={}",
                tx_hash,
                self.queue.len()
            );
        }
    }

    fn pop_ready_chunk(&mut self, n_txs: usize) -> Vec<TransactionReference> {
        // If get_ts() hasn't been called, return empty vec
        let Some(timestamp_threshold) = self.last_returned_timestamp else {
            debug!("FIFO pop_ready_chunk: get_ts() not called yet, returning empty");
            return Vec::new();
        };

        debug!(
            "FIFO pop_ready_chunk: n_txs={}, timestamp_threshold={}, queue_len={}",
            n_txs,
            timestamp_threshold,
            self.queue.len()
        );

        // Collect transactions that match the timestamp threshold
        let mut result = Vec::new();

        for &tx_hash in &self.queue {
            if result.len() >= n_txs {
                break;
            }

            if let Some(&tx_timestamp) = self.hash_to_timestamp.get(&tx_hash) {
                if tx_timestamp == timestamp_threshold {
                    if let Some(tx_ref) = self.hash_to_tx.remove(&tx_hash) {
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

        info!(
            "FIFO pop_ready_chunk: returned {} txs with timestamp={}, remaining_queue_len={}",
            result.len(),
            timestamp_threshold,
            self.queue.len()
        );

        result
    }

    fn remove_by_address(&mut self, _address: ContractAddress) -> bool {
        // FIFO queue doesn't support removal by address
        false
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

    fn get_nonce(&self, _address: ContractAddress) -> Option<Nonce> {
        // FIFO queue doesn't use get_nonce
        None
    }

    fn has_ready_txs(&self) -> bool {
        !self.queue.is_empty()
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
        next_txs_by_address: HashMap<ContractAddress, TransactionReference>,
        _validate_resource_bounds: bool,
    ) {
        // Rewind by re-inserting the next transaction for each address.
        for (address, tx_reference) in next_txs_by_address {
            self.remove_by_address(address);
            self.insert(tx_reference, false);
        }
    }

    fn priority_queue_len(&self) -> usize {
        self.queue.len()
    }

    fn pending_queue_len(&self) -> usize {
        0
    }

    fn get_first_tx_timestamp(&self) -> Option<UnixTimestamp> {
        self.queue.front().and_then(|hash| self.hash_to_timestamp.get(hash)).copied()
    }

    fn set_last_returned_timestamp(&mut self, timestamp: UnixTimestamp) {
        self.last_returned_timestamp = Some(timestamp);
    }

    fn get_last_returned_timestamp(&self) -> Option<UnixTimestamp> {
        self.last_returned_timestamp
    }

    fn update_timestamps(&mut self, mappings: HashMap<TransactionHash, UnixTimestamp>) {
        let count = mappings.len();
        info!("FIFO update_timestamps: received {} timestamp mappings", count);
        self.hash_to_timestamp.extend(mappings);
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
