use std::collections::{hash_map, BTreeMap, HashMap};

use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::executable_transaction::AccountTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_mempool_types::errors::MempoolError;
use starknet_mempool_types::mempool_types::{AccountState, MempoolResult};

use crate::mempool::TransactionReference;
use crate::utils::try_increment_nonce;

type HashToTransaction = HashMap<TransactionHash, AccountTransaction>;

/// Contains all transactions currently held in the mempool.
/// Invariant: both data structures are consistent regarding the existence of transactions:
/// A transaction appears in one if and only if it appears in the other.
/// No duplicate transactions appear in the pool.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct TransactionPool {
    // Holds the complete transaction objects; it should be the sole entity that does so.
    tx_pool: HashToTransaction,
    // Transactions organized by account address, sorted by ascending nonce values.
    txs_by_account: AccountTransactionIndex,
    // Tracks the capacity of the pool.
    capacity: PoolCapacity,
}

impl TransactionPool {
    pub fn insert(&mut self, tx: AccountTransaction) -> MempoolResult<()> {
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

        self.capacity.add();

        Ok(())
    }

    pub fn remove(&mut self, tx_hash: TransactionHash) -> MempoolResult<AccountTransaction> {
        // Remove from pool.
        let tx =
            self.tx_pool.remove(&tx_hash).ok_or(MempoolError::TransactionNotFound { tx_hash })?;

        // Remove from account mapping.
        self.txs_by_account.remove(TransactionReference::new(&tx)).unwrap_or_else(|| {
            panic!(
                "Transaction pool consistency error: transaction with hash {tx_hash} appears in
                main mapping, but does not appear in the account mapping"
            )
        });

        self.capacity.remove();

        Ok(tx)
    }

    pub fn remove_up_to_nonce(&mut self, address: ContractAddress, nonce: Nonce) {
        let removed_txs = self.txs_by_account.remove_up_to_nonce(address, nonce);

        for TransactionReference { tx_hash, .. } in removed_txs {
            self.tx_pool.remove(&tx_hash).unwrap_or_else(|| {
                panic!(
                    "Transaction pool consistency error: transaction with hash {tx_hash} appears
                    in account mapping, but does not appear in the main mapping"
                );
            });

            self.capacity.remove();
        }
    }

    pub fn account_txs_sorted_by_nonce(
        &self,
        address: ContractAddress,
    ) -> impl Iterator<Item = &TransactionReference> {
        self.txs_by_account.account_txs_sorted_by_nonce(address)
    }

    pub fn get_by_tx_hash(&self, tx_hash: TransactionHash) -> MempoolResult<&AccountTransaction> {
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

#[derive(Debug, Default, Eq, PartialEq)]
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
