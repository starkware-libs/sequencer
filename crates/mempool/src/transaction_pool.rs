use core::panic;
use std::collections::{hash_map, BTreeMap, HashMap};

use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::executable_transaction::Transaction;
use starknet_api::transaction::TransactionHash;
use starknet_mempool_types::errors::MempoolError;
use starknet_mempool_types::mempool_types::{
    Account,
    AccountState,
    MempoolResult,
    FEE_ESCALATION_THRESHOLD_DENOMINATOR,
    FEE_ESCALATION_THRESHOLD_NUMERATOR,
};

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
        if self.tx_pool.contains_key(&tx_hash) {
            return Err(MempoolError::DuplicateTransaction { tx_hash });
        }

        let address = tx_reference.sender_address;
        let nonce = tx_reference.nonce;
        if self.update_account_mapping(tx_reference) {
            self.insert_to_pool(tx_hash, tx)?;
            self.capacity.add();
            return Ok(());
        }
        Err(MempoolError::DuplicateNonce { address, nonce })
    }

    pub fn remove(&mut self, tx_hash: TransactionHash) -> MempoolResult<Transaction> {
        // Remove from pool.
        let tx =
            self.tx_pool.remove(&tx_hash).ok_or(MempoolError::TransactionNotFound { tx_hash })?;

        // Remove from account mapping.
        self.txs_by_account.remove(&TransactionReference::new(&tx)).unwrap_or_else(|| {
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
    pub fn n_txs(&self) -> usize {
        self.capacity.n_txs
    }

    pub fn contains_account(&self, address: ContractAddress) -> bool {
        self.txs_by_account.contains(address)
    }

    fn insert_to_pool(&mut self, tx_hash: TransactionHash, tx: Transaction) -> MempoolResult<()> {
        match self.tx_pool.entry(tx_hash) {
            hash_map::Entry::Vacant(entry) => {
                entry.insert(tx);
                Ok(())
            }
            hash_map::Entry::Occupied(_) => Err(MempoolError::DuplicateTransaction { tx_hash }),
        }
    }

    // fn update_account_mapping(&mut self, tx_reference: TransactionReference) -> bool {
    //     if let Some(current_tx) =
    //         self.txs_by_account.get(tx_reference.sender_address, tx_reference.nonce)
    //     {
    //         self.handle_existing_transaction(current_tx, tx_reference)
    //     } else {
    //         self.txs_by_account.insert(tx_reference).unwrap();
    //         true
    //     }
    // }

    fn update_account_mapping(&mut self, tx_reference: TransactionReference) -> bool {
        let sender_address = tx_reference.sender_address;
        let nonce = tx_reference.nonce;

        let transaction_exists = self.txs_by_account.get(sender_address, nonce).is_some();
        if transaction_exists {
            self.handle_existing_transaction(tx_reference)
        } else {
            assert_eq!(self.txs_by_account.insert(tx_reference), None);
            true
        }
    }

    fn handle_existing_transaction(&mut self, new_tx_ref: TransactionReference) -> bool {
        if let Some(current_tx) =
            self.txs_by_account.get(new_tx_ref.sender_address, new_tx_ref.nonce)
        {
            if self.should_replace_transaction(current_tx, &new_tx_ref) {
                self.txs_by_account.insert(new_tx_ref).unwrap();
                return true;
            }
        }
        false
    }

    fn should_replace_transaction(
        &self,
        current_tx: &TransactionReference,
        new_tx_ref: &TransactionReference,
    ) -> bool {
        let current_price = current_tx.get_l2_gas_price();
        let current_tip = current_tx.tip;
        let new_price = new_tx_ref.get_l2_gas_price();
        let new_tip = new_tx_ref.tip;

        new_price * FEE_ESCALATION_THRESHOLD_DENOMINATOR
            > current_price * FEE_ESCALATION_THRESHOLD_NUMERATOR
            && new_tip.0 * u64::try_from(FEE_ESCALATION_THRESHOLD_DENOMINATOR).unwrap()
                > current_tip.0 * u64::try_from(FEE_ESCALATION_THRESHOLD_NUMERATOR).unwrap()
    }

    // fn replace_transaction(
    //     &mut self,
    //     current_tx: &TransactionReference,
    //     new_tx_ref: TransactionReference,
    // ) { let tx_hash = current_tx.tx_hash; self.tx_pool.remove(&tx_hash).expect( "Transaction pool
    //   consistency error: transaction with hash {tx_hash} does not appear \ in main mapping, but
    //   it appears in the account mapping", );

    //     self.txs_by_account.insert(new_tx_ref);
    // }

    // fn remove_new_transaction(&mut self, tx_hash: &TransactionHash) -> MempoolResult<()> {
    //     self.tx_pool.remove(tx_hash).ok_or_else(|| {
    //         panic!(
    //             "Transaction pool consistency error: transaction with hash {tx_hash} does not \
    //              appear in main mapping, but it appears in the account mapping",
    //         )
    //     })?;
    //     Ok(())
    // }
}

#[derive(Debug, Default, Eq, PartialEq)]
struct AccountTransactionIndex(HashMap<ContractAddress, BTreeMap<Nonce, TransactionReference>>);

impl AccountTransactionIndex {
    /// If the transaction already exists in the mapping, the old value is returned.
    fn insert(&mut self, tx: TransactionReference) -> Option<TransactionReference> {
        self.0.entry(tx.sender_address).or_default().insert(tx.nonce, tx)
    }

    fn remove(&mut self, tx: &TransactionReference) -> Option<TransactionReference> {
        let TransactionReference { sender_address, nonce, .. } = tx;
        let account_txs = self.0.get_mut(sender_address)?;

        let removed_tx = account_txs.remove(nonce);

        if removed_tx.is_some() && account_txs.is_empty() {
            self.0.remove(sender_address);
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
