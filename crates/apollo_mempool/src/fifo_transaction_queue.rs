use std::collections::{HashMap, HashSet, VecDeque};

use apollo_mempool_types::mempool_types::TransactionQueueSnapshot;
use indexmap::IndexSet;
use starknet_api::block::{GasPrice, UnixTimestamp};
use starknet_api::core::{ContractAddress, Nonce};
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
    // Map from transaction hash to timestamp. Entries are kept after pop for potential rewind,
    // and cleaned up after commit via remove_txs.
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
        // Note: hash_to_timestamp is NOT removed here because timestamps are kept for potential
        // rewind. Use delete_timestamps() after commit to clean up committed transaction
        // timestamps.
        let mut removed_txs = Vec::new();
        for tx in txs {
            if let Some(tx_ref) = self.hash_to_tx.remove(&tx.tx_hash) {
                if let Some(pos) = self.queue.iter().position(|&hash| hash == tx.tx_hash) {
                    self.queue.remove(pos);
                }
                removed_txs.push(tx_ref);
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
        let (staged_tx_refs, committed_nonces, rejected_tx_hashes) = match rewind_data {
            RewindData::Fifo { staged_tx_refs, committed_nonces, rejected_tx_hashes } => {
                (staged_tx_refs, committed_nonces, rejected_tx_hashes)
            }
            RewindData::FeePriority { .. } => {
                panic!("FifoTransactionQueue received FeePriority data instead of Fifo data")
            }
        };

        let should_rewind_address = |txs: &[&TransactionReference],
                                     committed_nonce: Option<&Nonce>,
                                     rejected_tx_hashes: &IndexSet<TransactionHash>|
         -> bool {
            if let Some(&nonce) = committed_nonce {
                // Address has committed transactions: find the tx at committed_nonce (the
                // "following" tx).
                // - If following tx doesn't exist → rewind (gap: committed some, didn't stage next)
                // - If following tx is rejected → don't rewind (entire chain rejected, drop all)
                // - If following tx is not rejected → rewind (will retry non-committed txs)
                txs.iter()
                    .find(|tx| tx.nonce == nonce)
                    .is_none_or(|following_tx| !rejected_tx_hashes.contains(&following_tx.tx_hash))
            } else {
                // Address has NO committed transactions: find the first tx (lowest nonce).
                // - If first tx is rejected → don't rewind (chain head rejected, drop all)
                // - If first tx is not rejected → rewind (will retry all txs)
                let first_tx = txs
                    .iter()
                    .min_by_key(|tx| tx.nonce)
                    .expect("staged_by_address entry must have at least one transaction");
                !rejected_tx_hashes.contains(&first_tx.tx_hash)
            }
        };

        let mut rewind_tx = |tx_ref: TransactionReference| {
            let tx_hash = tx_ref.tx_hash;
            let timestamp = *self
                .hash_to_timestamp
                .get(&tx_hash)
                .expect("Rewound transaction must have a timestamp already set");
            debug!(
                "FIFO rewind: tx_hash={}, timestamp={}, queue_before={:?}",
                tx_hash, timestamp, self.queue
            );
            // Add to FRONT of queue so rewound txs are processed before new txs.
            self.queue.push_front(tx_hash);
            self.hash_to_tx.insert(tx_hash, tx_ref);
        };

        // Group staged transactions by address
        let staged_by_address: HashMap<ContractAddress, Vec<&TransactionReference>> =
            staged_tx_refs.iter().fold(HashMap::new(), |mut acc, tx| {
                acc.entry(tx.address).or_default().push(tx);
                acc
            });

        // First pass: Determine which addresses should have their non-committed txs rewound.
        let addresses_to_rewind: HashSet<ContractAddress> = staged_by_address
            .iter()
            .filter(|(&address, txs)| {
                should_rewind_address(txs, committed_nonces.get(&address), rejected_tx_hashes)
            })
            .map(|(&address, _)| address)
            .collect();

        // Second pass: Rewind non-committed transactions for selected addresses.
        // We iterate in reverse to maintain FIFO order when pushing to front of queue.
        let mut rewound_hashes = IndexSet::new();

        for tx_ref in staged_tx_refs.iter().rev() {
            // Skip if address doesn't need rewinding
            if !addresses_to_rewind.contains(&tx_ref.address) {
                continue;
            }

            // Skip if transaction was committed (tx_ref.nonce < committed_nonce for this address).
            // Committed txs were already removed from tx_pool, so we don't rewind them.
            let is_committed =
                committed_nonces.get(&tx_ref.address).is_some_and(|&cn| tx_ref.nonce < cn);
            if is_committed {
                continue;
            }

            // Track rewound transaction hash.
            rewound_hashes.insert(tx_ref.tx_hash);
            rewind_tx(*tx_ref);
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

    fn delete_timestamp(&mut self, tx_hashes: &[TransactionHash]) {
        for tx_hash in tx_hashes {
            self.hash_to_timestamp.remove(tx_hash);
        }
    }
}
