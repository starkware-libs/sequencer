use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::Arc;

use apollo_mempool_config::config::{MempoolConfig, MempoolDynamicConfig};
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
use apollo_time::time::Clock;
use indexmap::IndexSet;
use starknet_api::block::GasPrice;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::rpc_transaction::InternalRpcTransaction;
use starknet_api::transaction::fields::Tip;
use starknet_api::transaction::TransactionHash;
use tracing::{debug, error, info};

// HACK: Tests commented out - they test the old complex Mempool implementation
// TODO(Fix): Update tests to work with naive FIFO mempool or create new test suite
// #[cfg(test)]
// #[path = "mempool_test.rs"]
// pub mod mempool_test;
//
// #[cfg(test)]
// #[path = "mempool_flow_tests.rs"]
// pub mod mempool_flow_tests;

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
pub struct Mempool {
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

impl Mempool {
    pub fn new(_config: MempoolConfig, _clock: Arc<dyn Clock>) -> Self {
        Self::default()
    }

    /// Retrieves up to `n_txs` transactions from the mempool in FIFO order.
    pub fn get_txs(&mut self, n_txs: usize) -> MempoolResult<Vec<InternalRpcTransaction>> {
        debug!(
            "Mempool: get_txs called: requested={}, queue_size={}, pool_size={}",
            n_txs,
            self.queue.len(),
            self.tx_pool.len()
        );

        let take_count = n_txs.min(self.queue.len());
        debug!("Mempool: get_txs: taking {} transactions from queue", take_count);

        let tx_hashes: Vec<TransactionHash> = self.queue.drain(..take_count).collect();
        debug!(
            "Mempool: get_txs: drained {} transaction hashes from queue: {:?}",
            tx_hashes.len(),
            tx_hashes
        );

        // Track staged transactions (sent to batcher) for rewind logic
        // Store in order to preserve FIFO when rewinding
        for tx_hash in &tx_hashes {
            self.staged_txs.push_back(*tx_hash);
        }

        // Transactions are NOT removed from the mempool until `commit_block` is called.
        let result: Vec<InternalRpcTransaction> = tx_hashes
            .iter()
            .map(|hash| {
                debug!("Mempool: get_txs: fetching tx_hash={} from tx_pool", hash);
                match self.tx_pool.get(hash) {
                    Some(tx) => tx.clone(),
                    None => {
                        error!(
                            "Mempool: BUG: Transaction hash {} in queue but not found in tx_pool! \
                             queue_size={}, pool_size={}",
                            hash,
                            self.queue.len(),
                            self.tx_pool.len()
                        );
                        panic!("Transaction hash in queue must exist in tx_pool: {}", hash);
                    }
                }
            })
            .collect();

        let n_returned_txs = result.len();
        debug!(
            "Mempool: get_txs: returning {} transactions, remaining queue_size={}, staged_txs={}, \
             pool_size={}",
            n_returned_txs,
            self.queue.len(),
            self.staged_txs.len(),
            self.tx_pool.len()
        );
        if n_returned_txs != 0 {
            info!(
                "Mempool: Returned {n_returned_txs} out of {n_txs} transactions from mempool, \
                 ready for sequencing."
            );
        } else {
            debug!("Mempool: get_txs: no transactions returned (queue empty or requested 0)");
        }

        Ok(result)
    }

    /// Adds a transaction to the mempool.
    pub fn add_tx(&mut self, args: AddTransactionArgs) -> MempoolResult<()> {
        let tx_hash = args.tx.tx_hash();
        let address = args.tx.contract_address();
        let tx_nonce = args.tx.nonce();
        debug!(
            "Mempool: add_tx: received tx_hash={}, address={}, tx_nonce={}, \
             current_queue_size={}, current_pool_size={}",
            tx_hash,
            address,
            tx_nonce,
            self.queue.len(),
            self.tx_pool.len()
        );

        self.queue.push_back(tx_hash);
        debug!(
            "Mempool: add_tx: pushed tx_hash={} to queue, new_queue_size={}",
            tx_hash,
            self.queue.len()
        );

        self.tx_pool.insert(tx_hash, args.tx);
        debug!(
            "Mempool: add_tx: inserted tx_hash={} to tx_pool, new_pool_size={}",
            tx_hash,
            self.tx_pool.len()
        );

        // Add to account index
        self.txs_by_account.entry(address).or_default().insert(tx_nonce, tx_hash);
        debug!(
            "Mempool: add_tx: added tx_hash={} to txs_by_account[address={}][nonce={}], \
             total_accounts={}",
            tx_hash,
            address,
            tx_nonce,
            self.txs_by_account.len()
        );

        // Verify transaction is actually in queue and pool
        let in_queue = self.queue.contains(&tx_hash);
        let in_pool = self.tx_pool.contains_key(&tx_hash);

        debug!(
            "Mempool: add_tx: Transaction successfully added to mempool: tx_hash={}, address={}, \
             tx_nonce={}, final_queue_size={}, final_pool_size={}, in_queue={}, in_pool={}",
            tx_hash,
            address,
            tx_nonce,
            self.queue.len(),
            self.tx_pool.len(),
            in_queue,
            in_pool
        );

        if !in_queue || !in_pool {
            error!(
                "Mempool: BUG: Transaction {} not properly stored! in_queue={}, in_pool={}, \
                 queue_size={}, pool_size={}",
                tx_hash,
                in_queue,
                in_pool,
                self.queue.len(),
                self.tx_pool.len()
            );
        }

        info!(
            "Mempool: add_tx: SUCCESS - tx_hash={} added, queue_size={}, pool_size={}, \
             verified_in_queue={}, verified_in_pool={}",
            tx_hash,
            self.queue.len(),
            self.tx_pool.len(),
            in_queue,
            in_pool
        );

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
            "Mempool: commit_block: with {} addresses, {} rejected txs, queue_size={}, \
             staged_txs={}, pool_size={}",
            address_to_nonce.len(),
            rejected_tx_hashes.len(),
            self.queue.len(),
            self.staged_txs.len(),
            self.tx_pool.len()
        );

        // Step 1: Remove committed transactions and create list of committed tx hashes
        let (committed_addresses, committed_tx_hashes) =
            self.remove_committed_txs(&address_to_nonce);

        // Step 2: Rewind staged transactions that need rewinding
        let (_addresses_to_rewind, rejected_txs_to_skip) =
            self.rewind_staged_txs(&committed_addresses, &committed_tx_hashes, &rejected_tx_hashes);

        // Step 3: Remove rejected transactions that were not rewound
        self.remove_rejected_txs(&rejected_tx_hashes, &rejected_txs_to_skip);

        debug!(
            "Mempool: commit_block: commit_block complete: final_queue_size={}, final_pool_size={}",
            self.queue.len(),
            self.tx_pool.len()
        );
    }

    /// Removes committed transactions from tx_pool and txs_by_account.
    /// Returns the set of committed addresses and committed transaction hashes.
    fn remove_committed_txs(
        &mut self,
        address_to_nonce: &HashMap<ContractAddress, Nonce>,
    ) -> (HashSet<ContractAddress>, HashSet<TransactionHash>) {
        debug!("Mempool: commit_block: processing {} committed addresses", address_to_nonce.len());
        let committed_addresses: HashSet<ContractAddress> =
            address_to_nonce.keys().copied().collect();
        let mut committed_tx_hashes = HashSet::new();

        for (&address, &next_nonce) in address_to_nonce {
            debug!(
                "Mempool: commit_block: removing committed txs for address={} up to next_nonce={}",
                address, next_nonce
            );
            let removed_txs = self.remove_up_to_nonce(address, next_nonce);
            debug!(
                "Mempool: commit_block: removed {} committed transactions for address={}: {:?}",
                removed_txs.len(),
                address,
                removed_txs
            );
            committed_tx_hashes.extend(removed_txs.iter().copied());
        }

        (committed_addresses, committed_tx_hashes)
    }

    /// Rewinds staged transactions that need rewinding.
    /// Returns the set of addresses that need rewinding and rejected tx hashes that were rewound.
    fn rewind_staged_txs(
        &mut self,
        committed_addresses: &HashSet<ContractAddress>,
        committed_tx_hashes: &HashSet<TransactionHash>,
        rejected_tx_hashes: &IndexSet<TransactionHash>,
    ) -> (HashSet<ContractAddress>, HashSet<TransactionHash>) {
        let mut addresses_to_rewind = HashSet::new();
        let mut rejected_txs_to_skip = HashSet::new();

        debug!("Mempool: commit_block: checking {} staged transactions", self.staged_txs.len());

        // Iterate staged_txs in reverse to preserve FIFO order when pushing to front
        // staged_txs is in FIFO order [oldest, ..., newest]
        // To preserve FIFO when pushing to front, we need to push newest first, then oldest
        for tx_hash in self.staged_txs.drain(..).rev() {
            // Skip if transaction was already removed (committed)
            if committed_tx_hashes.contains(&tx_hash) {
                debug!("Mempool: commit_block: skipping rewinding tx_hash={} (committed)", tx_hash);
                continue;
            }

            // Get transaction address from pool
            // Staged transactions should always be in tx_pool (they were added via get_txs)
            let tx = self.tx_pool.get(&tx_hash).expect(
                "Staged transaction must exist in tx_pool. This indicates a bug in the mempool.",
            );
            let address = tx.contract_address();

            // If address is NOT committed
            if !committed_addresses.contains(&address) {
                // Add address to "address_to_rewind"
                addresses_to_rewind.insert(address);
                // Add tx back to queue
                self.queue.push_front(tx_hash);
                // If this transaction is also rejected, mark it to skip removal in step 3
                if rejected_tx_hashes.contains(&tx_hash) {
                    rejected_txs_to_skip.insert(tx_hash);
                }
                debug!(
                    "Mempool: commit_block: rewound tx_hash={} (address={} not committed)",
                    tx_hash, address
                );
            } else {
                // Address IS committed, check if tx hash is in rejected txs
                if rejected_tx_hashes.contains(&tx_hash) {
                    // Check if address is in "address_to_rewind"
                    if addresses_to_rewind.contains(&address) {
                        // Add tx back to queue
                        self.queue.push_front(tx_hash);
                        // Remove tx from rejected txs (mark to skip in step 3)
                        rejected_txs_to_skip.insert(tx_hash);
                        debug!(
                            "Mempool: commit_block: rewound rejected tx_hash={} (address={} needs \
                             rewind)",
                            tx_hash, address
                        );
                    }
                }
            }
        }

        (addresses_to_rewind, rejected_txs_to_skip)
    }

    /// Removes rejected transactions that were not rewound from tx_pool and txs_by_account.
    /// Rejected transactions are not in the queue (they were removed by get_txs), so we don't need
    /// to remove them from the queue.
    fn remove_rejected_txs(
        &mut self,
        rejected_tx_hashes: &IndexSet<TransactionHash>,
        rejected_txs_to_skip: &HashSet<TransactionHash>,
    ) {
        debug!(
            "Mempool: commit_block: processing {} rejected transactions",
            rejected_tx_hashes.len()
        );
        for tx_hash in rejected_tx_hashes {
            // Skip if this transaction was already marked to skip (rewound in step 2)
            if rejected_txs_to_skip.contains(tx_hash) {
                debug!(
                    "Mempool: commit_block: skipping removal of rejected tx_hash={} (was rewound \
                     in step 2)",
                    tx_hash
                );
                continue;
            }

            // Remove rejected transaction from tx_pool and txs_by_account
            debug!("Mempool: commit_block: removing rejected tx_hash={}", tx_hash);
            if let Some(tx) = self.tx_pool.remove(tx_hash) {
                let address = tx.contract_address();
                let tx_nonce = tx.nonce();
                debug!(
                    "Mempool: commit_block: removed rejected tx_hash={} from tx_pool (address={}, \
                     nonce={})",
                    tx_hash, address, tx_nonce
                );
                if let Some(account_txs) = self.txs_by_account.get_mut(&address) {
                    account_txs.remove(&tx_nonce);
                    debug!(
                        "Mempool: commit_block: removed rejected tx_hash={} from \
                         txs_by_account[address={}][nonce={}]",
                        tx_hash, address, tx_nonce
                    );
                    // Only remove account entry if it's empty AND the rejected tx was nonce 0
                    // (DeployAccount). For nonce > 0, keep the entry even if empty to handle
                    // race conditions where account_tx_in_pool_or_recent_block is called before
                    // state is updated.
                    if account_txs.is_empty() && tx_nonce == Nonce::default() {
                        self.txs_by_account.remove(&address);
                        debug!(
                            "Mempool: commit_block: removed empty account entry for address={} \
                             (rejected DeployAccount with nonce 0)",
                            address
                        );
                    } else if account_txs.is_empty() {
                        debug!(
                            "Mempool: commit_block: keeping empty account entry for address={} \
                             (rejected tx had nonce > 0, account exists)",
                            address
                        );
                    }
                }
            } else {
                debug!("Mempool: commit_block: rejected tx_hash={} not found in tx_pool", tx_hash);
            }
        }
    }

    /// Validates a transaction (checks for duplicate hash).
    ///
    /// This is called by Gateway before add_tx as part of the API interface.
    /// Part of the API contract - Gateway calls this to check if transaction can be added.
    pub fn validate_tx(&mut self, args: ValidationArgs) -> MempoolResult<()> {
        debug!(
            "Mempool: validate_tx: checking tx_hash={}, pool_size={}",
            args.tx_hash,
            self.tx_pool.len()
        );
        if self.tx_pool.contains_key(&args.tx_hash) {
            debug!("Mempool: validate_tx: duplicate tx_hash={} found in pool", args.tx_hash);
            return Err(MempoolError::DuplicateTransaction { tx_hash: args.tx_hash });
        }
        debug!("Mempool: validate_tx: tx_hash={} is valid (not duplicate)", args.tx_hash);
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
        debug!("Mempool: remove_up_to_nonce: address={}, next_nonce={}", address, next_nonce);
        let Some(account_txs) = self.txs_by_account.get_mut(&address) else {
            debug!("Mempool: remove_up_to_nonce: address={} not found in txs_by_account", address);
            return Vec::default();
        };

        debug!(
            "Mempool: remove_up_to_nonce: address={} has {} transactions before removal",
            address,
            account_txs.len()
        );

        // Split the transactions at the given next_nonce (same pattern as transaction_pool.rs).
        // split_off returns everything >= next_nonce, so we keep >= next_nonce and remove <
        // next_nonce.
        let txs_with_higher_or_equal_nonce = account_txs.split_off(&next_nonce);
        let txs_with_lower_nonce = std::mem::replace(account_txs, txs_with_higher_or_equal_nonce);

        debug!(
            "Mempool: remove_up_to_nonce: address={} will remove {} transactions (nonce < {}), \
             keeping {} transactions",
            address,
            txs_with_lower_nonce.len(),
            next_nonce,
            account_txs.len()
        );

        // Don't remove empty account entry for committed transactions.
        // This allows account_tx_in_pool_or_recent_block to return true even after all
        // transactions are committed, handling race conditions where the Gateway checks
        // before state is updated.
        if account_txs.is_empty() {
            debug!(
                "Mempool: remove_up_to_nonce: keeping empty account entry for address={} (all \
                 transactions committed, but account exists)",
                address
            );
        }

        // Collect transaction hashes to remove (nonce < next_nonce)
        let txs_to_remove: Vec<TransactionHash> = txs_with_lower_nonce.into_values().collect();
        debug!(
            "Mempool: remove_up_to_nonce: collected {} transaction hashes to remove: {:?}",
            txs_to_remove.len(),
            txs_to_remove
        );

        // Remove from tx_pool
        for tx_hash in &txs_to_remove {
            if self.tx_pool.remove(tx_hash).is_some() {
                debug!("Mempool: remove_up_to_nonce: removed tx_hash={} from tx_pool", tx_hash);
            } else {
                debug!(
                    "Mempool: remove_up_to_nonce: tx_hash={} not found in tx_pool (already \
                     removed?)",
                    tx_hash
                );
            }
        }

        debug!(
            "Mempool: remove_up_to_nonce: completed for address={}, removed {} transactions",
            address,
            txs_to_remove.len()
        );
        txs_to_remove
    }
}

/// Provides a lightweight representation of a transaction for mempool usage (e.g., excluding
/// execution fields).
/// TODO(Mohammad): rename this struct to `ThinTransaction` once that name
/// becomes available, to better reflect its purpose and usage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransactionReference {
    pub address: ContractAddress,
    pub nonce: Nonce,
    pub tx_hash: TransactionHash,
    pub tip: Tip,
    pub max_l2_gas_price: GasPrice,
}

impl TransactionReference {
    pub fn new(tx: &InternalRpcTransaction) -> Self {
        TransactionReference {
            address: tx.contract_address(),
            nonce: tx.nonce(),
            tx_hash: tx.tx_hash(),
            tip: tx.tip(),
            max_l2_gas_price: tx.resource_bounds().l2_gas.max_price_per_unit,
        }
    }
}

impl From<&ValidationArgs> for TransactionReference {
    fn from(args: &ValidationArgs) -> Self {
        TransactionReference {
            address: args.address,
            nonce: args.tx_nonce,
            tx_hash: args.tx_hash,
            tip: args.tip,
            max_l2_gas_price: args.max_l2_gas_price,
        }
    }
}

impl std::fmt::Display for TransactionReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let TransactionReference { address, nonce, tx_hash, tip, max_l2_gas_price } = self;
        write!(
            f,
            "TransactionReference {{ address: {address}, nonce: {nonce}, tx_hash: {tx_hash}, tip: \
             {tip}, max_l2_gas_price: {max_l2_gas_price} }}"
        )
    }
}
