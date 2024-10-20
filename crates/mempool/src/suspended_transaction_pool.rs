use std::collections::HashMap;

use starknet_api::core::{ContractAddress, Nonce};

use crate::mempool::TransactionReference;

type _AddressNonceToTransaction = HashMap<(ContractAddress, Nonce), TransactionReference>;

#[derive(Debug)]
pub struct _SuspendedTransactionPool {
    suspended_tx_pool: _AddressNonceToTransaction,
}

impl _SuspendedTransactionPool {
    pub fn _contains(&self, address: ContractAddress, nonce: Nonce) -> bool {
        self.suspended_tx_pool.contains_key(&(address, nonce))
    }

    pub fn _insert(&mut self, tx: TransactionReference) {
        assert_eq!(
            self.suspended_tx_pool.insert((tx.address, tx.nonce), tx),
            None,
            "Keys should be unique; duplicates are checked prior."
        );
    }

    pub fn _remove(&mut self, tx: &TransactionReference) -> bool {
        self.suspended_tx_pool.remove(&(tx.address, tx.nonce)).is_some()
    }
}
