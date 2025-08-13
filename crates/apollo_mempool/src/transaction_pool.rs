use std::cmp::Ordering;
use std::collections::{hash_map, BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use apollo_mempool_types::errors::MempoolError;
use apollo_mempool_types::mempool_types::{AccountState, MempoolResult};
use apollo_metrics::metrics::MetricHistogram;
use apollo_time::time::{Clock, DateTime};
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::rpc_transaction::InternalRpcTransaction;
use starknet_api::transaction::TransactionHash;

use crate::mempool::TransactionReference;
use crate::metrics::{TRANSACTION_TIME_SPENT_IN_MEMPOOL, TRANSACTION_TIME_SPENT_UNTIL_COMMITTED};
use crate::utils::try_increment_nonce;

#[cfg(test)]
#[path = "transaction_pool_test.rs"]
pub mod transaction_pool_test;

type HashToTransaction = HashMap<TransactionHash, InternalRpcTransaction>;

/// Contains all transactions currently held in the mempool.
/// Invariant: all data structures are consistent regarding the existence of transactions:
/// A transaction appears in one if and only if it appears in the other.
/// No duplicate transactions appear in the pool.
pub struct TransactionPool {
    // Holds the complete transaction objects; it should be the sole entity that does so.
    tx_pool: HashToTransaction,
    // Transactions organized by account address, sorted by ascending nonce values.
    txs_by_account: AccountTransactionIndex,
    // Transactions sorted by their time spent in the pool (i.e. newest to oldest).
    txs_by_submission_time: TimedTransactionMap,
    // Tracks the size of the pool.
    size: PoolSize,
}

impl TransactionPool {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        TransactionPool {
            tx_pool: HashMap::new(),
            txs_by_account: AccountTransactionIndex::default(),
            txs_by_submission_time: TimedTransactionMap::new(clock),
            size: PoolSize::default(),
        }
    }

    pub fn len(&self) -> usize {
        self.tx_pool.len()
    }

    pub fn size_in_bytes(&self) -> u64 {
        self.size.size_in_bytes()
    }

    pub fn insert(&mut self, tx: InternalRpcTransaction) -> MempoolResult<()> {
        let tx_reference = TransactionReference::new(&tx);
        let tx_hash = tx_reference.tx_hash;
        let tx_size = tx.total_bytes();

        // Insert to pool.
        if let hash_map::Entry::Vacant(entry) = self.tx_pool.entry(tx_hash) {
            entry.insert(tx);
        } else {
            return Err(MempoolError::DuplicateTransaction { tx_hash });
        }

        // Insert to account mapping.
        let unexpected_existing_tx = self.txs_by_account.insert(tx_reference);
        if unexpected_existing_tx.is_some() {
            panic!(
                "Transaction pool consistency error: transaction with hash {tx_hash} does not
                appear in main mapping, but transaction with same nonce appears in the account
                mapping",
            )
        };

        // Insert to timed mapping.
        let unexpected_existing_tx = self.txs_by_submission_time.insert(tx_reference);
        if unexpected_existing_tx.is_some() {
            panic!(
                "Transaction pool consistency error: transaction with hash {tx_hash} does not
                appear in main mapping, but transaction with same hash appears in the timed
                mapping",
            )
        };

        self.size.add(tx_size);

        Ok(())
    }

    pub fn remove(&mut self, tx_hash: TransactionHash) -> MempoolResult<InternalRpcTransaction> {
        // Remove from pool.
        let tx =
            self.tx_pool.remove(&tx_hash).ok_or(MempoolError::TransactionNotFound { tx_hash })?;

        // Remove reference from other mappings.
        let removed_tx = vec![TransactionReference::new(&tx)];
        self.remove_from_account_mapping(&removed_tx);
        self.remove_from_timed_mapping(&removed_tx);

        self.size.remove(tx.total_bytes());

        Ok(tx)
    }

    // Note: Use this function only for commit flow. Using elsewhere will record incorrect commit
    // times.
    pub fn remove_up_to_nonce_when_committed(
        &mut self,
        address: ContractAddress,
        nonce: Nonce,
    ) -> usize {
        let removed_txs = self.txs_by_account.remove_up_to_nonce(address, nonce);

        for tx_ref in &removed_txs {
            let submission_time = self
                .get_submission_time(tx_ref.tx_hash)
                .expect("Transaction must still be in Mempool when recording commit latency");
            self.txs_by_submission_time
                .record_time_spent(submission_time, &TRANSACTION_TIME_SPENT_UNTIL_COMMITTED);
        }

        self.remove_from_main_mapping(&removed_txs);
        self.remove_from_timed_mapping(&removed_txs);

        removed_txs.len()
    }

    pub fn remove_txs_older_than(
        &mut self,
        duration: Duration,
        exclude_txs: &HashMap<ContractAddress, Nonce>,
    ) -> Vec<TransactionReference> {
        let removed_txs = self.txs_by_submission_time.remove_txs_older_than(duration, exclude_txs);

        self.remove_from_main_mapping(&removed_txs);
        self.remove_from_account_mapping(&removed_txs);

        removed_txs
    }

    pub fn account_txs_sorted_by_nonce(
        &self,
        address: ContractAddress,
    ) -> impl Iterator<Item = &TransactionReference> {
        self.txs_by_account.account_txs_sorted_by_nonce(address)
    }

    pub fn get_by_tx_hash(
        &self,
        tx_hash: TransactionHash,
    ) -> MempoolResult<&InternalRpcTransaction> {
        self.tx_pool.get(&tx_hash).ok_or(MempoolError::TransactionNotFound { tx_hash })
    }

    pub fn get_by_address_and_nonce(
        &self,
        address: ContractAddress,
        nonce: Nonce,
    ) -> Option<TransactionReference> {
        self.txs_by_account.get(address, nonce)
    }

    pub fn get_next_eligible_tx(
        &self,
        current_account_state: AccountState,
    ) -> MempoolResult<Option<TransactionReference>> {
        let AccountState { address, nonce } = current_account_state;
        let next_nonce = try_increment_nonce(nonce)?;
        Ok(self.get_by_address_and_nonce(address, next_nonce))
    }

    pub fn contains_account(&self, address: ContractAddress) -> bool {
        self.txs_by_account.contains(address)
    }

    pub fn get_submission_time(&self, tx_hash: TransactionHash) -> MempoolResult<DateTime> {
        self.txs_by_submission_time
            .hash_to_submission_id
            .get(&tx_hash)
            .map(|submission_id| submission_id.submission_time)
            .ok_or(MempoolError::TransactionNotFound { tx_hash })
    }

    pub fn get_lowest_nonce(&self, address: ContractAddress) -> Option<Nonce> {
        self.account_txs_sorted_by_nonce(address).next().map(|tx_ref| tx_ref.nonce)
    }

    fn remove_from_main_mapping(&mut self, removed_txs: &Vec<TransactionReference>) {
        for TransactionReference { tx_hash, .. } in removed_txs {
            let tx = self.tx_pool.remove(tx_hash).unwrap_or_else(|| {
                panic!(
                    "Transaction pool consistency error: transaction with hash {tx_hash} does not \
                     appear in the main mapping.",
                )
            });
            self.size.remove(tx.total_bytes());
        }
    }

    fn remove_from_account_mapping(&mut self, removed_txs: &Vec<TransactionReference>) {
        for tx in removed_txs {
            let tx_hash = tx.tx_hash;
            self.txs_by_account.remove(*tx).unwrap_or_else(|| {
                panic!(
                    "Transaction pool consistency error: transaction with hash {tx_hash} does not \
                     appear in the account index mapping.",
                )
            });
        }
    }

    fn remove_from_timed_mapping(&mut self, removed_txs: &Vec<TransactionReference>) {
        for TransactionReference { tx_hash, .. } in removed_txs {
            self.txs_by_submission_time.remove(*tx_hash).unwrap_or_else(|| {
                panic!(
                    "Transaction pool consistency error: transaction with hash {tx_hash} does not \
                     appear in the timed mapping.",
                )
            });
        }
    }

    pub fn chronological_txs_hashes(&self) -> Vec<TransactionHash> {
        self.txs_by_submission_time
            .txs_by_submission_time
            .keys()
            .map(|submission_id| submission_id.tx_hash)
            .collect()
    }

    #[cfg(test)]
    pub fn tx_pool(&self) -> HashMap<TransactionHash, InternalRpcTransaction> {
        self.tx_pool.clone()
    }
}

#[derive(Debug, Default, Eq, PartialEq)]
struct AccountTransactionIndex(HashMap<ContractAddress, BTreeMap<Nonce, TransactionReference>>);

impl AccountTransactionIndex {
    /// If the transaction already exists in the mapping, the old value is returned.
    fn insert(&mut self, tx: TransactionReference) -> Option<TransactionReference> {
        self.0.entry(tx.address).or_default().insert(tx.nonce, tx)
    }

    fn remove(&mut self, tx: TransactionReference) -> Option<TransactionReference> {
        let TransactionReference { address, nonce, .. } = tx;
        let account_txs = self.0.get_mut(&address)?;

        let removed_tx = account_txs.remove(&nonce);

        if removed_tx.is_some() && account_txs.is_empty() {
            self.0.remove(&address);
        }

        removed_tx
    }

    fn get(&self, address: ContractAddress, nonce: Nonce) -> Option<TransactionReference> {
        self.0.get(&address)?.get(&nonce).copied()
    }

    fn account_txs_sorted_by_nonce(
        &self,
        address: ContractAddress,
    ) -> impl Iterator<Item = &TransactionReference> {
        self.0.get(&address).into_iter().flat_map(|nonce_to_tx_ref| nonce_to_tx_ref.values())
    }

    fn remove_up_to_nonce(
        &mut self,
        address: ContractAddress,
        nonce: Nonce,
    ) -> Vec<TransactionReference> {
        let Some(account_txs) = self.0.get_mut(&address) else {
            return Vec::default();
        };

        // Split the transactions at the given nonce.
        let txs_with_higher_or_equal_nonce = account_txs.split_off(&nonce);
        let txs_with_lower_nonce = std::mem::replace(account_txs, txs_with_higher_or_equal_nonce);

        if account_txs.is_empty() {
            self.0.remove(&address);
        }

        // Collect and return the transactions with lower nonces.
        txs_with_lower_nonce.into_values().collect()
    }

    fn contains(&self, address: ContractAddress) -> bool {
        self.0.contains_key(&address)
    }
}

#[derive(Debug, Default, Eq, PartialEq, Clone)]
pub struct PoolSize {
    // Keeps track of the total size of the transactions in the pool.
    size_in_bytes: u64,
}

impl PoolSize {
    fn add(&mut self, tx_size_in_bytes: u64) {
        self.size_in_bytes = self
            .size_in_bytes
            .checked_add(tx_size_in_bytes)
            .expect("Overflow when adding to PoolCapacity size_in_bytes.");
    }

    fn remove(&mut self, tx_size_in_bytes: u64) {
        self.size_in_bytes = self
            .size_in_bytes
            .checked_sub(tx_size_in_bytes)
            .expect("Underflow when subtracting from PoolCapacity size_in_bytes.");
    }

    fn size_in_bytes(&self) -> u64 {
        self.size_in_bytes
    }
}

/// Uniquely identify a transaction submission.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SubmissionID {
    submission_time: DateTime,
    tx_hash: TransactionHash,
}

// Implementing the `Ord` trait based on the transaction's duration in the pool. I.e. a transaction
// that has been in the pool for a longer time will be considered "greater" than a transaction that
// has been in the pool for a shorter time.
impl Ord for SubmissionID {
    fn cmp(&self, other: &Self) -> Ordering {
        self.submission_time
            .cmp(&other.submission_time)
            .reverse()
            .then_with(|| self.tx_hash.cmp(&other.tx_hash))
    }
}

impl PartialOrd for SubmissionID {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct TimedTransactionMap {
    txs_by_submission_time: BTreeMap<SubmissionID, TransactionReference>,
    hash_to_submission_id: HashMap<TransactionHash, SubmissionID>,
    clock: Arc<dyn Clock>,
}

impl TimedTransactionMap {
    fn new(clock: Arc<dyn Clock>) -> Self {
        TimedTransactionMap {
            txs_by_submission_time: BTreeMap::new(),
            hash_to_submission_id: HashMap::new(),
            clock,
        }
    }

    /// If a transaction with the same transaction hash already exists in the mapping, the previous
    /// submission ID is returned.
    fn insert(&mut self, tx: TransactionReference) -> Option<SubmissionID> {
        let submission_id = SubmissionID { submission_time: self.clock.now(), tx_hash: tx.tx_hash };
        self.txs_by_submission_time.insert(submission_id.clone(), tx);
        self.hash_to_submission_id.insert(tx.tx_hash, submission_id)
    }

    /// Removes the transaction with the given transaction hash from the mapping.
    /// Returns the removed transaction reference if it exists in the mapping.
    fn remove(&mut self, tx_hash: TransactionHash) -> Option<TransactionReference> {
        let submission_id = self.hash_to_submission_id.remove(&tx_hash)?;
        self.record_time_spent(submission_id.submission_time, &TRANSACTION_TIME_SPENT_IN_MEMPOOL);
        self.txs_by_submission_time.remove(&submission_id)
    }

    /// Removes all transactions that were submitted to the pool before the given duration.
    /// Transactions for accounts listed in exclude_txs with nonces lower than the specified nonce
    /// are preserved.
    pub fn remove_txs_older_than(
        &mut self,
        duration: Duration,
        exclude_txs: &HashMap<ContractAddress, Nonce>,
    ) -> Vec<TransactionReference> {
        let split_off_value = SubmissionID {
            submission_time: self.clock.now() - duration,
            tx_hash: Default::default(),
        };
        let old_txs = self.txs_by_submission_time.split_off(&split_off_value);

        let mut removed_txs = Vec::new();
        for (submission_id, tx) in old_txs.into_iter() {
            if exclude_txs.get(&tx.address).is_some_and(|nonce| tx.nonce < *nonce) {
                // The transaction should be preserved. Add it back.
                self.txs_by_submission_time.insert(submission_id, tx);
            } else {
                let submission_id = self.hash_to_submission_id.remove(&tx.tx_hash).expect(
                    "Transaction should have a submission ID if it is in the timed transaction \
                     map.",
                );
                removed_txs.push(tx);

                self.record_time_spent(
                    submission_id.submission_time,
                    &TRANSACTION_TIME_SPENT_IN_MEMPOOL,
                );
            }
        }

        removed_txs
    }

    fn record_time_spent(&self, submission_time: DateTime, metric: &MetricHistogram) {
        let time_spent_secs = (self.clock.now() - submission_time).to_std().unwrap().as_secs_f64();
        metric.record(time_spent_secs);
    }
}
