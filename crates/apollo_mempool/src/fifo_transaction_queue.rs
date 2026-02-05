use std::collections::{HashMap, HashSet, VecDeque};

use apollo_mempool_types::mempool_types::TransactionQueueSnapshot;
use starknet_api::block::GasPrice;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::transaction::TransactionHash;
use tracing::{debug, info, warn};
use url::Url;

use crate::mempool::TransactionReference;
use crate::transaction_queue_trait::{RewindData, TransactionQueueTrait};

/// FIFO transaction queue implementation.
/// Stores transactions in insertion order and returns them in FIFO order.
pub struct FifoTransactionQueue {
    queue: VecDeque<TransactionHash>,
    hash_to_tx: HashMap<TransactionHash, TransactionReference>,
    hash_to_timestamp: HashMap<TransactionHash, u64>,
    last_returned_timestamp: Option<u64>,
    recorder_url: Url,
}

impl FifoTransactionQueue {
    pub fn new(recorder_url: Url) -> Self {
        Self {
            queue: VecDeque::new(),
            hash_to_tx: HashMap::new(),
            hash_to_timestamp: HashMap::new(),
            last_returned_timestamp: None,
            recorder_url,
        }
    }
}

impl TransactionQueueTrait for FifoTransactionQueue {
    fn insert(&mut self, tx_reference: TransactionReference, _validate_resource_bounds: bool) {
        let tx_hash = tx_reference.tx_hash;

        tracing::debug!("FIFO insert: tx_hash={}, queue_len_before={}", tx_hash, self.queue.len());

        // Add transaction to queue in FIFO order
        self.queue.push_back(tx_hash);
        self.hash_to_tx.insert(tx_hash, tx_reference);

        // Fetch timestamp from echonet API (BLOCKING call - waits for HTTP response)
        // This is called during add_tx() flow, so transaction insertion blocks on HTTP call
        tracing::debug!("FIFO insert: fetching timestamp for tx_hash={}", tx_hash);
        let timestamp = self.fetch_tx_timestamp(tx_hash);
        tracing::info!(
            "FIFO insert: tx_hash={}, timestamp={}, queue_len={}",
            tx_hash,
            timestamp,
            self.queue.len()
        );
        self.hash_to_timestamp.insert(tx_hash, timestamp);
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
                        self.hash_to_timestamp.remove(&tx_hash);
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

    fn update_gas_price_threshold(&mut self, _threshold: GasPrice) {
        // No-op for FIFO queue
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

    fn rewind_txs(&mut self, rewind_data: RewindData) -> Option<HashSet<TransactionHash>> {
        let RewindData::Fifo { staged_tx_refs, committed_nonces, rejected_tx_hashes } = rewind_data
        else {
            panic!("FIFO queue requires RewindData::Fifo variant");
        };

        // Group transactions by address
        let staged_by_address: HashMap<ContractAddress, Vec<&TransactionReference>> =
            staged_tx_refs.iter().fold(HashMap::new(), |mut acc, tx| {
                acc.entry(tx.address).or_default().push(tx);
                acc
            });

        let mut rewound_hashes = HashSet::new();
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
                // Rewind: re-insert at front (preserve FIFO order)
                debug!("FIFO rewind: re-inserting tx_hash={} at front of queue", tx_ref.tx_hash);
                let timestamp = self.fetch_tx_timestamp(tx_ref.tx_hash);
                self.hash_to_timestamp.insert(tx_ref.tx_hash, timestamp);
                debug!(
                    "FIFO rewind: fetched timestamp={} for tx_hash={}",
                    timestamp, tx_ref.tx_hash
                );
                self.queue.push_front(tx_ref.tx_hash);
                self.hash_to_tx.insert(tx_ref.tx_hash, *tx_ref);
                rewound_hashes.insert(tx_ref.tx_hash);
                already_rewound.insert(tx_ref.tx_hash);
            }
        }

        Some(rewound_hashes)
    }

    fn priority_queue_len(&self) -> usize {
        // FIFO queue doesn't distinguish priority/pending, so return total queue length.
        self.queue.len()
    }

    fn pending_queue_len(&self) -> usize {
        0
    }

    fn get_first_tx_timestamp(&self) -> Option<u64> {
        self.queue.front().and_then(|hash| self.hash_to_timestamp.get(hash)).copied()
    }

    fn set_last_returned_timestamp(&mut self, timestamp: u64) {
        self.last_returned_timestamp = Some(timestamp);
    }
}

impl FifoTransactionQueue {
    /// Fetches the timestamp for a transaction from the echonet API.
    ///
    /// Makes a synchronous (blocking) HTTP GET request to:
    ///   {recorder_url}/echonet/tx_timestamp?transactionHash={tx_hash}
    ///
    /// Expected response: {"timestamp": 1234} or just 1234
    ///
    /// Retries once on any failure (network error, non-200 status, JSON parse error).
    /// Panics if both attempts fail - this is a critical error as we cannot proceed without
    /// timestamp.
    fn fetch_tx_timestamp(&self, tx_hash: TransactionHash) -> u64 {
        let url = self
            .recorder_url
            .join(&format!("echonet/tx_timestamp?transactionHash={}", tx_hash))
            .expect("Failed to construct timestamp URL");

        debug!("FIFO fetch_tx_timestamp: tx_hash={}, url={}", tx_hash, url);

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("Failed to create HTTP client");

        // Try twice (initial attempt + 1 retry)
        for attempt in 0..2 {
            debug!("FIFO fetch_tx_timestamp: attempt {} for tx_hash={}", attempt + 1, tx_hash);

            match client.get(url.as_str()).send() {
                Ok(response) if response.status().is_success() => {
                    // Success response - try to parse JSON
                    if let Ok(json) = response.json::<serde_json::Value>() {
                        // Try to parse timestamp from JSON: {"timestamp": 1234} or just 1234
                        if let Some(timestamp) =
                            json.get("timestamp").and_then(|v| v.as_u64()).or_else(|| json.as_u64())
                        {
                            info!(
                                "FIFO fetch_tx_timestamp: SUCCESS tx_hash={}, timestamp={}",
                                tx_hash, timestamp
                            );
                            return timestamp;
                        }
                        warn!(
                            "FIFO fetch_tx_timestamp: JSON parse failed for tx_hash={}, json={:?}",
                            tx_hash, json
                        );
                    }
                    // JSON parse failed - will retry if attempt == 0
                }
                Ok(response) => {
                    // Non-success HTTP status (4xx, 5xx) - will retry if attempt == 0
                    warn!(
                        "FIFO fetch_tx_timestamp: HTTP error for tx_hash={}, status={}",
                        tx_hash,
                        response.status()
                    );
                }
                Err(e) => {
                    // Network error - will retry if attempt == 0
                    warn!(
                        "FIFO fetch_tx_timestamp: Network error for tx_hash={}, error={}",
                        tx_hash, e
                    );
                }
            }
            // Loop continues to retry (if attempt == 0)
        }

        // Both attempts failed
        panic!(
            "CRITICAL: Failed to fetch timestamp for tx {} from {} after 2 attempts. Cannot \
             proceed.",
            tx_hash, url
        );
    }

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
