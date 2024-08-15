use std::collections::{hash_map, BTreeMap, HashMap};

use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::executable_transaction::Transaction;
use starknet_api::transaction::TransactionHash;
use starknet_mempool_types::errors::MempoolError;
use starknet_mempool_types::mempool_types::{Account, AccountState, MempoolResult};

use crate::mempool::TransactionReference;

type HashToTransaction = HashMap<TransactionHash, Transaction>;

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
    pub fn insert(&mut self, tx: Transaction) -> MempoolResult<()> {
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
                "Transaction pool consistency error: transaction with hash {tx_hash} does not \
                 appear in main mapping, but it appears in the account mapping",
            )
        };

        self.capacity.add();

        Ok(())
    }

    pub fn remove(&mut self, tx_hash: TransactionHash) -> MempoolResult<Transaction> {
        // Remove from pool.
        let tx =
            self.tx_pool.remove(&tx_hash).ok_or(MempoolError::TransactionNotFound { tx_hash })?;

        // Remove from account mapping.
        self.txs_by_account.remove(TransactionReference::new(&tx)).unwrap_or_else(|| {
            panic!(
                "Transaction pool consistency error: transaction with hash {tx_hash} appears in \
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
                    "Transaction pool consistency error: transaction with hash {tx_hash} appears \
                     in account mapping, but does not appear in the main mapping"
                );
            });
            self.capacity.remove();
        }
    }

    pub fn _get_by_tx_hash(&self, tx_hash: TransactionHash) -> MempoolResult<&Transaction> {
        self.tx_pool.get(&tx_hash).ok_or(MempoolError::TransactionNotFound { tx_hash })
    }

    pub fn get_by_address_and_nonce(
        &self,
        address: ContractAddress,
        nonce: Nonce,
    ) -> Option<&TransactionReference> {
        self.txs_by_account.get(address, nonce)
    }

    pub fn get_next_eligible_tx(
        &self,
        current_account_state: Account,
    ) -> MempoolResult<Option<&TransactionReference>> {
        let Account { sender_address, state: AccountState { nonce } } = current_account_state;
        // TOOD(Ayelet): Change to StarknetApiError.
        let next_nonce = nonce.try_increment().map_err(|_| MempoolError::FeltOutOfRange)?;
        Ok(self.get_by_address_and_nonce(sender_address, next_nonce))
    }

    #[cfg(test)]
    pub(crate) fn _tx_pool(&self) -> &HashToTransaction {
        &self.tx_pool
    }

    #[cfg(test)]
    pub fn txs_count(&self) -> usize {
        self.capacity.txs_counter
    }
}

#[derive(Debug, Default, Eq, PartialEq)]
struct AccountTransactionIndex(HashMap<ContractAddress, BTreeMap<Nonce, TransactionReference>>);

impl AccountTransactionIndex {
    /// If the transaction already exists in the mapping, the old value is returned.
    fn insert(&mut self, tx: TransactionReference) -> Option<TransactionReference> {
        self.0.entry(tx.sender_address).or_default().insert(tx.nonce, tx)
    }

    fn remove(&mut self, tx: TransactionReference) -> Option<TransactionReference> {
        let TransactionReference { sender_address, nonce, .. } = tx;
        let account_txs = self.0.get_mut(&sender_address)?;

        let removed_tx = account_txs.remove(&nonce);

        if removed_tx.is_some() && account_txs.is_empty() {
            self.0.remove(&sender_address);
        }

        removed_tx
    }

    fn get(&self, address: ContractAddress, nonce: Nonce) -> Option<&TransactionReference> {
        self.0.get(&address)?.get(&nonce)
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
}

#[derive(Debug, Default, Eq, PartialEq)]
pub struct PoolCapacity {
    txs_counter: usize,
    // TODO(Ayelet): Add size tracking.
}

impl PoolCapacity {
    fn add(&mut self) {
        self.txs_counter += 1;
    }

    fn remove(&mut self) {
        if self.txs_counter > 0 {
            self.txs_counter -= 1;
        }
    }
}
