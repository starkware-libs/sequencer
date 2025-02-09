use std::cmp::Ordering;
use std::collections::{hash_map, BTreeMap, HashMap};
use std::time::{Duration, Instant};

use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::rpc_transaction::InternalRpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_mempool_types::errors::MempoolError;
use starknet_mempool_types::mempool_types::{AccountState, MempoolResult};

use crate::mempool::TransactionReference;
use crate::utils::try_increment_nonce;

type HashToTransaction = HashMap<TransactionHash, InternalRpcTransaction>;

/// Contains all transactions currently held in the mempool.
/// Invariant: both data structures are consistent regarding the existence of transactions:
/// A transaction appears in one if and only if it appears in the other.
/// No duplicate transactions appear in the pool.
#[derive(Debug, Default)]
pub struct TransactionPool {
    // Holds the complete transaction objects; it should be the sole entity that does so.
    tx_pool: HashToTransaction,
    // Transactions organized by account address, sorted by ascending nonce values.
    txs_by_account: AccountTransactionIndex,
    // Transactions sorted by their time spent in the pool, i.e., in descending order of submission
    // time.
    txs_by_submission_time: TimedTransactionMap,
    // Tracks the capacity of the pool.
    capacity: PoolCapacity,
}

impl TransactionPool {
    pub fn insert(&mut self, tx: InternalRpcTransaction) -> MempoolResult<()> {
        let tx_reference = TransactionReference::new(&tx);
        let tx_hash = tx_reference.tx_hash;

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

        self.capacity.add();

        Ok(())
    }

    pub fn remove(&mut self, tx_hash: TransactionHash) -> MempoolResult<InternalRpcTransaction> {
        // Remove from pool.
        let tx =
            self.tx_pool.remove(&tx_hash).ok_or(MempoolError::TransactionNotFound { tx_hash })?;

        // Remove reference from other mappings.
        let removed_tx = vec![TransactionReference::new(&tx)];
        self.align_other_mappings_with(&removed_tx, PoolMappings::Main);

        Ok(tx)
    }

    pub fn remove_up_to_nonce(&mut self, address: ContractAddress, nonce: Nonce) {
        let removed_txs = self.txs_by_account.remove_up_to_nonce(address, nonce);
        self.align_other_mappings_with(&removed_txs, PoolMappings::AccountTransactionIndex);
    }

    #[allow(dead_code)]
    pub fn remove_txs_older_than(&mut self, duration: Duration) -> Vec<TransactionReference> {
        let removed_txs = self.txs_by_submission_time.remove_txs_older_than(duration);
        self.align_other_mappings_with(&removed_txs, PoolMappings::TimedTransactionMap);

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

    pub fn _contains_account(&self, address: ContractAddress) -> bool {
        self.txs_by_account._contains(address)
    }

    // Rmoves the given `removed_txs` from all the pool mappings, except for the one specified in
    // `skip_mapping`.
    fn align_other_mappings_with(
        &mut self,
        removed_txs: &Vec<TransactionReference>,
        skip_mapping: PoolMappings,
    ) {
        for tx in removed_txs {
            let tx_hash = tx.tx_hash;

            if skip_mapping != PoolMappings::Main {
                self.tx_pool.remove(&tx_hash).unwrap_or_else(|| {
                    panic!(
                        "Transaction pool consistency error: transaction with hash {tx_hash} does \
                         not appear in the main mapping.",
                    )
                });
            }

            if skip_mapping != PoolMappings::AccountTransactionIndex {
                self.txs_by_account.remove(*tx).unwrap_or_else(|| {
                    panic!(
                        "Transaction pool consistency error: transaction with hash {tx_hash} does \
                         not appear in the account index mapping.",
                    )
                });
            }

            if skip_mapping != PoolMappings::TimedTransactionMap {
                self.txs_by_submission_time.remove(tx_hash).unwrap_or_else(|| {
                    panic!(
                        "Transaction pool consistency error: transaction with hash {tx_hash} does \
                         not appear in the timed mapping.",
                    )
                });
            }

            self.capacity.remove();
        }
    }

    #[cfg(test)]
    pub fn content(&self) -> TransactionPoolContent {
        TransactionPoolContent { tx_pool: self.tx_pool.clone() }
    }
}

#[cfg(test)]
#[derive(Debug, Default, PartialEq, Eq)]
pub struct TransactionPoolContent {
    pub tx_pool: HashMap<TransactionHash, InternalRpcTransaction>,
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

    fn _contains(&self, address: ContractAddress) -> bool {
        self.0.contains_key(&address)
    }
}

#[derive(Debug, Default, Eq, PartialEq, Clone)]
pub struct PoolCapacity {
    n_txs: usize,
    // TODO(Ayelet): Add size tracking.
}

impl PoolCapacity {
    fn add(&mut self) {
        self.n_txs += 1;
    }

    fn remove(&mut self) {
        self.n_txs =
            self.n_txs.checked_sub(1).expect("Underflow: Cannot subtract from an empty pool.");
    }
}

#[derive(Eq, PartialEq)]
enum PoolMappings {
    Main,
    AccountTransactionIndex,
    TimedTransactionMap,
}

/// Uniquly identify a transaction submission.
#[derive(Clone, Debug)]
struct SubmissionID {
    submission_time: Instant,
    tx_hash: TransactionHash,
}

impl PartialEq for SubmissionID {
    fn eq(&self, other: &Self) -> bool {
        self.submission_time == other.submission_time && self.tx_hash == other.tx_hash
    }
}

impl Eq for SubmissionID {}

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

#[derive(Debug, Default, Eq, PartialEq)]
struct TimedTransactionMap {
    txs_by_submission_time: BTreeMap<SubmissionID, TransactionReference>,
    hash_to_submission_id: HashMap<TransactionHash, SubmissionID>,
}

impl TimedTransactionMap {
    /// If the transaction with the same transaction hash already exists in the mapping, the old
    /// submission ID is returned.
    fn insert(&mut self, tx: TransactionReference) -> Option<SubmissionID> {
        // TODO(dafna, 1/3/2025): Use a Clock trait instead of Instant.
        let submission_id = SubmissionID { submission_time: Instant::now(), tx_hash: tx.tx_hash };
        self.txs_by_submission_time.insert(submission_id.clone(), tx);
        self.hash_to_submission_id.insert(tx.tx_hash, submission_id)
    }

    /// Removes the transaction with the given transaction hash from the mapping.
    /// Returns the removed transaction reference if it exists in the mapping.
    fn remove(&mut self, tx_hash: TransactionHash) -> Option<TransactionReference> {
        let submission_id = self.hash_to_submission_id.remove(&tx_hash)?;
        self.txs_by_submission_time.remove(&submission_id)
    }

    /// Removes all transactions that were submitted to the pool before the given duration.
    #[allow(dead_code)]
    pub fn remove_txs_older_than(&mut self, duration: Duration) -> Vec<TransactionReference> {
        let split_off_value = SubmissionID {
            submission_time: Instant::now() - duration,
            tx_hash: Default::default(),
        };
        let removed_txs: Vec<_> =
            self.txs_by_submission_time.split_off(&split_off_value).into_values().collect();

        for tx in removed_txs.iter() {
            self.hash_to_submission_id.remove(&tx.tx_hash).expect(
                "Transaction should have a submission ID if it is in the timed transaction map.",
            );
        }

        removed_txs
    }
}
