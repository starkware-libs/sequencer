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
    // Transactions that were returned by get_txs and may need rewind during commit.
    staged_tx_refs: Vec<TransactionReference>,
}

impl FifoTransactionQueue {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            hash_to_tx: HashMap::new(),
            hash_to_timestamp: HashMap::new(),
            last_returned_timestamp: None,
            staged_tx_refs: Vec::new(),
        }
    }

    fn group_staged_txs_by_address(
        &self,
        staged_tx_refs: &[TransactionReference],
    ) -> HashMap<ContractAddress, Vec<TransactionReference>> {
        let mut grouped_by_address: HashMap<ContractAddress, Vec<TransactionReference>> =
            HashMap::new();
        for &tx in staged_tx_refs {
            grouped_by_address.entry(tx.address).or_default().push(tx);
        }
        grouped_by_address
    }

    fn collect_txs_to_rewind(
        &self,
        committed_nonces: &HashMap<ContractAddress, Nonce>,
        rejected_tx_hashes: &IndexSet<TransactionHash>,
    ) -> Vec<TransactionReference> {
        // Step 1: group staged txs by address so rewind policy is evaluated per account.
        let staged_by_address = self.group_staged_txs_by_address(&self.staged_tx_refs);
        // Step 2: decide which addresses should be rewound based on committed nonce + rejections.
        let addresses_to_rewind: HashSet<ContractAddress> = staged_by_address
            .iter()
            .filter(|(address, txs)| {
                if let Some(&nonce) = committed_nonces.get(address) {
                    // Address has committed txs in this block. if the next nonce is:
                    // - missing -> rewind this address
                    // - present + rejected -> do not rewind this address
                    // - present + not rejected -> rewind this address
                    txs.iter().find(|tx| tx.nonce == nonce).is_none_or(|following_tx| {
                        !rejected_tx_hashes.contains(&following_tx.tx_hash)
                    })
                } else {
                    // Address has no committed txs in this block.
                    // Use first nonce to decide if the address should be rewound:
                    // - first nonce rejected -> do not rewind address
                    // - first nonce not rejected -> rewind address
                    let first_tx = txs
                        .iter()
                        .min_by_key(|tx| tx.nonce)
                        .expect("staged_by_address entry must have at least one transaction");
                    !rejected_tx_hashes.contains(&first_tx.tx_hash)
                }
            })
            .map(|(&address, _)| address)
            .collect();

        if addresses_to_rewind.is_empty() {
            return Vec::new();
        }

        // Step 3: staged txs to rewind: keep addresses marked for rewind, excluding txs already
        // committed in this block (nonce < committed nonce)
        self.staged_tx_refs
            .iter()
            .rev()
            .filter(|tx_ref| addresses_to_rewind.contains(&tx_ref.address))
            .filter(|tx_ref| {
                committed_nonces
                    .get(&tx_ref.address)
                    .is_none_or(|&committed_nonce| tx_ref.nonce >= committed_nonce)
            })
            .copied()
            .collect()
    }

    fn delete_timestamps_for_committed_txs(&mut self, rewound_hashes: &IndexSet<TransactionHash>) {
        for tx_ref in &self.staged_tx_refs {
            if rewound_hashes.contains(&tx_ref.tx_hash) {
                continue;
            }
            self.hash_to_timestamp.remove(&tx_ref.tx_hash);
        }
    }

    fn rewind_tx(&mut self, tx_ref: TransactionReference) {
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
        // If get_timestamp() hasn't been called, return empty vec
        let Some(timestamp_threshold) = self.last_returned_timestamp else {
            return Vec::new();
        };

        // Collect transactions that match the timestamp threshold
        let mut result = Vec::with_capacity(n_txs);
        while result.len() < n_txs {
            let Some(tx_hash) = self.queue.front().copied() else {
                break;
            };

            let Some(&tx_timestamp) = self.hash_to_timestamp.get(&tx_hash) else {
                break;
            };

            if tx_timestamp != timestamp_threshold {
                break;
            }

            self.queue.pop_front();

            if let Some(tx_ref) = self.hash_to_tx.remove(&tx_hash) {
                debug!(
                    "FIFO pop_ready_chunk: popping tx_hash={}, timestamp={}, \
                     last_returned_timestamp={:?}",
                    tx_hash, tx_timestamp, self.last_returned_timestamp
                );
                result.push(tx_ref);
                // Keep timestamp in map for potential rewind.
            }
        }

        result
    }

    fn remove_txs(&mut self, txs: &[TransactionReference]) -> Vec<TransactionReference> {
        // Note: hash_to_timestamp is NOT removed here because timestamps are kept for potential
        // rewind. Use delete_timestamps() after commit to clean up committed transaction
        // timestamps.
        let tx_hashes: HashSet<TransactionHash> = txs.iter().map(|tx| tx.tx_hash).collect();

        let mut removed_txs = Vec::with_capacity(tx_hashes.len());

        for hash in &tx_hashes {
            if let Some(tx_ref) = self.hash_to_tx.remove(hash) {
                removed_txs.push(tx_ref);
            }
        }

        self.queue.retain(|h| !tx_hashes.contains(h));

        removed_txs
    }

    fn has_ready_txs(&self) -> bool {
        // If get_timestamp() hasn't been called yet, no txs are ready
        let Some(timestamp_threshold) = self.last_returned_timestamp else {
            return false;
        };

        // Check if the first tx in queue has the same timestamp as last_returned_timestamp
        if let Some(first_hash) = self.queue.front() {
            return self
                .hash_to_timestamp
                .get(first_hash)
                .is_some_and(|&ts| ts == timestamp_threshold);
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

    fn rewind_txs(&mut self, rewind_data: RewindData<'_>) -> IndexSet<TransactionHash> {
        // Extract FIFO-specific data
        let RewindData::Fifo { committed_nonces, rejected_tx_hashes } = rewind_data else {
            unreachable!("FifoTransactionQueue received FeePriority data instead of Fifo data");
        };

        let txs_to_rewind = self.collect_txs_to_rewind(committed_nonces, rejected_tx_hashes);
        let rewound_hashes: IndexSet<TransactionHash> = txs_to_rewind
            .into_iter()
            .map(|tx| {
                let tx_hash = tx.tx_hash;
                self.rewind_tx(tx);
                tx_hash
            })
            .collect();

        self.delete_timestamps_for_committed_txs(&rewound_hashes);
        self.staged_tx_refs.clear();

        rewound_hashes
    }

    fn stage_txs_for_rewind(&mut self, txs: &[TransactionReference]) {
        self.staged_tx_refs.extend(txs.iter().copied());
    }

    fn priority_queue_len(&self) -> usize {
        self.queue.len()
    }

    fn pending_queue_len(&self) -> usize {
        0
    }

    fn resolve_timestamp(&mut self) -> UnixTimestamp {
        // If queue is non-empty, use front tx timestamp and persist it as current threshold.
        if let Some(timestamp) =
            self.queue.front().and_then(|hash| self.hash_to_timestamp.get(hash)).copied()
        {
            self.last_returned_timestamp = Some(timestamp);
            return timestamp;
        }
        // If queue is empty, reuse last returned threshold.
        // If no threshold was ever returned, default to 0.
        self.last_returned_timestamp.unwrap_or(0)
    }

    fn update_timestamp(&mut self, tx_hash: TransactionHash, timestamp: UnixTimestamp) {
        self.hash_to_timestamp.insert(tx_hash, timestamp);
    }
}
