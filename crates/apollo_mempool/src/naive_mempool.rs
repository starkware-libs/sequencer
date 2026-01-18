use std::collections::{BTreeMap, HashMap, VecDeque};

use apollo_mempool_config::config::MempoolDynamicConfig;
use apollo_mempool_types::errors::MempoolError;
use apollo_mempool_types::mempool_types::{
    AddTransactionArgs,
    CommitBlockArgs,
    MempoolResult,
    MempoolSnapshot,
    MempoolStateSnapshot,
    TransactionQueueSnapshot,
    ValidationArgs,
};
use starknet_api::block::GasPrice;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::rpc_transaction::InternalRpcTransaction;
use starknet_api::transaction::TransactionHash;
use tracing::{debug, info};

use crate::mempool::TransactionReference;

/// A minimal FIFO mempool implementation.
///
/// This implementation assumes all transactions were already accepted in blocks,
/// so it skips validation, fee logic, replacement rules, and complex state management.
///
/// Key simplifications:
/// - Simple FIFO queue (VecDeque) for transaction ordering
/// - HashMap for transaction storage
/// - No priority queues, fee escalation, or capacity management
/// - No nonce tracking or gap management
/// - No TTL/expiration logic
/// - Simple rewind logic: transactions returned by `get_txs()` but not committed are put back in
///   queue
#[derive(Default)]
pub struct NaiveMempool {
    /// FIFO queue of transaction hashes in order of arrival
    queue: VecDeque<TransactionHash>,
    /// Map from transaction hash to full transaction
    tx_pool: HashMap<TransactionHash, InternalRpcTransaction>,
    /// Transactions organized by account address, sorted by ascending nonce values.
    txs_by_account: HashMap<ContractAddress, BTreeMap<Nonce, TransactionHash>>,
    /// Transactions that were sent to batcher (popped from queue in get_txs) for rewind logic
    /// Stored as VecDeque to preserve FIFO order when rewinding
    staged_txs: VecDeque<TransactionHash>,
}

impl NaiveMempool {
    pub fn new() -> Self {
        Self::default()
    }

    /// Retrieves up to `n_txs` transactions from the mempool in FIFO order.
    pub fn get_txs(&mut self, n_txs: usize) -> MempoolResult<Vec<InternalRpcTransaction>> {
        let take_count = n_txs.min(self.queue.len());

        let tx_hashes: Vec<TransactionHash> = self.queue.drain(..take_count).collect();

        // Track staged transactions (sent to batcher) for rewind logic
        // Store in order to preserve FIFO when rewinding
        for tx_hash in &tx_hashes {
            self.staged_txs.push_back(*tx_hash);
        }

        // Transactions are NOT removed from the mempool until `commit_block` is called.
        let result: Vec<InternalRpcTransaction> = tx_hashes
            .iter()
            .map(|hash| {
                self.tx_pool
                    .get(hash)
                    .expect("Transaction hash in queue must exist in tx_pool")
                    .clone()
            })
            .collect();

        let n_returned_txs = result.len();
        if n_returned_txs != 0 {
            info!(
                "Returned {n_returned_txs} out of {n_txs} transactions from naive mempool, ready \
                 for sequencing."
            );
        }

        Ok(result)
    }

    /// Adds a transaction to the mempool.
    pub fn add_tx(&mut self, args: AddTransactionArgs) -> MempoolResult<()> {
        let tx_hash = args.tx.tx_hash();
        let address = args.tx.contract_address();
        let tx_nonce = args.tx.nonce();
        info!("Adding transaction to naive mempool.");
        debug!(
            "Transaction added to naive mempool: tx_hash={}, address={}, tx_nonce={}",
            tx_hash, address, tx_nonce
        );
        self.queue.push_back(tx_hash);
        self.tx_pool.insert(tx_hash, args.tx);

        // Add to account index
        self.txs_by_account.entry(address).or_default().insert(tx_nonce, tx_hash);

        Ok(())
    }

    /// Updates the mempool state after a block is committed.
    ///
    /// Removes:
    /// 1. Rejected transactions (by hash) - these are in the transactions map (not queue, since
    ///    they were removed from queue by get_txs, but kept in map for soft-delete pattern)
    /// 2. Committed transactions (by address and nonce <= committed nonce)
    ///
    /// Rewinds:
    /// 3. Transactions that were returned by `get_txs()` but not committed are put back in queue
    ///    (rewind logic). This ensures they can be returned again in future `get_txs()` calls.
    ///
    /// Note: `address_to_nonce` tells us which addresses had transactions committed and their
    /// final nonce. We iterate through transactions to find matching address/nonce pairs and
    /// remove those with `nonce <= committed_nonce`. This is O(n) but acceptable for naive
    /// implementation.
    pub fn commit_block(&mut self, args: CommitBlockArgs) {
        let CommitBlockArgs { address_to_nonce, rejected_tx_hashes } = args;

        debug!(
            "Committing block with {} addresses and {} rejected txs to naive mempool.",
            address_to_nonce.len(),
            rejected_tx_hashes.len()
        );

        // Track which popped transactions were committed or rejected
        use std::collections::HashSet;
        let mut committed_or_rejected_txs = HashSet::new();

        // Remove rejected transactions from tx_pool, txs_by_account, and queue.
        for tx_hash in &rejected_tx_hashes {
            committed_or_rejected_txs.insert(*tx_hash);
            if let Some(tx) = self.tx_pool.remove(tx_hash) {
                let address = tx.contract_address();
                let tx_nonce = tx.nonce();
                if let Some(account_txs) = self.txs_by_account.get_mut(&address) {
                    account_txs.remove(&tx_nonce);
                    if account_txs.is_empty() {
                        self.txs_by_account.remove(&address);
                    }
                }
            }
            // Also remove from queue if still there (not staged)
            self.queue.retain(|&h| h != *tx_hash);
        }

        // Remove committed transactions from tx_pool, txs_by_account, and queue.
        // The nonce passed is the "next_nonce" (nonce after the last committed transaction).
        // So if next_nonce is 4, we remove transactions with nonce < 4 (i.e., 0, 1, 2, 3).
        for (&address, &next_nonce) in &address_to_nonce {
            let removed_txs = self.remove_up_to_nonce(address, next_nonce);
            committed_or_rejected_txs.extend(removed_txs.iter().copied());
            // Also remove from queue if they're still there (not staged)
            for tx_hash in &removed_txs {
                self.queue.retain(|&h| h != *tx_hash);
            }
        }

        // Rewind logic: Put transactions back in queue if they were sent to batcher
        // but not committed/rejected. Preserve FIFO order.
        for tx_hash in self.staged_txs.drain(..) {
            // Skip if committed or rejected (already removed from tx_pool)
            if committed_or_rejected_txs.contains(&tx_hash) {
                continue;
            }

            // Put back in queue (rewind) - transaction is still in tx_pool
            self.queue.push_back(tx_hash);
        }
    }

    /// Validates a transaction (checks for duplicate hash).
    ///
    /// This is called by Gateway before add_tx as part of the API interface.
    /// Part of the API contract - Gateway calls this to check if transaction can be added.
    pub fn validate_tx(&mut self, args: ValidationArgs) -> MempoolResult<()> {
        if self.tx_pool.contains_key(&args.tx_hash) {
            return Err(MempoolError::DuplicateTransaction { tx_hash: args.tx_hash });
        }
        Ok(())
    }

    /// Returns a snapshot of the mempool state.
    ///
    /// Called by monitoring endpoint for debugging/monitoring.
    /// Returns simplified snapshot (empty delayed_declares, empty queues, empty nonce state).
    pub fn mempool_snapshot(&self) -> MempoolResult<MempoolSnapshot> {
        Ok(MempoolSnapshot {
            transactions: self.queue.iter().copied().collect(),
            delayed_declares: Vec::new(),
            transaction_queue: TransactionQueueSnapshot {
                gas_price_threshold: GasPrice::default(),
                priority_queue: Vec::new(),
                pending_queue: Vec::new(),
            },
            mempool_state: MempoolStateSnapshot {
                committed: HashMap::new(),
                staged: HashMap::new(),
            },
        })
    }

    /// Updates the gas price threshold (no-op for naive mempool).
    ///
    /// Called by consensus/orchestrator when gas price changes.
    /// Part of API interface, but no-op since we don't use gas price logic.
    pub fn update_gas_price(&mut self, _threshold: GasPrice) {
        // No-op: naive mempool doesn't use gas price logic
    }

    /// Checks if an account has transactions in the mempool.
    ///
    /// Called by Gateway during validation (e.g., to check if deploy_account tx exists).
    /// Part of the API interface - Gateway uses this to determine if account has pending txs.
    /// Note: We only check mempool (not "recent blocks" since we don't track that).
    pub fn account_tx_in_pool_or_recent_block(&self, account_address: ContractAddress) -> bool {
        self.txs_by_account.contains_key(&account_address)
    }

    /// Updates dynamic config (no-op for naive mempool, kept for interface compatibility).
    pub fn update_dynamic_config(&mut self, _mempool_dynamic_config: MempoolDynamicConfig) {
        // No-op: naive mempool doesn't use dynamic config
    }

    /// Returns an iterator over transactions (for interface compatibility).
    pub fn iter(&self) -> impl Iterator<Item = TransactionReference> + '_ {
        self.queue.iter().filter_map(|hash| self.tx_pool.get(hash)).map(TransactionReference::new)
    }

    /// Removes all transactions for the given address with nonce < the given nonce
    /// from both tx_pool and txs_by_account.
    /// The nonce parameter is the "next_nonce" (nonce after the last committed transaction).
    /// Returns the transaction hashes that were removed.
    fn remove_up_to_nonce(
        &mut self,
        address: ContractAddress,
        next_nonce: Nonce,
    ) -> Vec<TransactionHash> {
        let Some(account_txs) = self.txs_by_account.get_mut(&address) else {
            return Vec::default();
        };

        // Split the transactions at the given next_nonce (same pattern as transaction_pool.rs).
        // split_off returns everything >= next_nonce, so we keep >= next_nonce and remove <
        // next_nonce.
        let txs_with_higher_or_equal_nonce = account_txs.split_off(&next_nonce);
        let txs_with_lower_nonce = std::mem::replace(account_txs, txs_with_higher_or_equal_nonce);

        // Clean up empty address entry
        if account_txs.is_empty() {
            self.txs_by_account.remove(&address);
        }

        // Collect transaction hashes to remove (nonce < next_nonce)
        let txs_to_remove: Vec<TransactionHash> = txs_with_lower_nonce.into_values().collect();

        // Remove from tx_pool
        for tx_hash in &txs_to_remove {
            self.tx_pool.remove(tx_hash);
        }

        txs_to_remove
    }
}
