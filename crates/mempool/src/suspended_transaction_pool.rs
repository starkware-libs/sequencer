use std::collections::HashMap;

use starknet_api::core::{ContractAddress, Nonce};

use crate::mempool::TransactionReference;

type _AddressNonceToTransaction = HashMap<(ContractAddress, Nonce), TransactionReference>;

#[derive(Debug)]
pub struct _SuspendedTransactionPool {
    suspended_tx_pool: _AddressNonceToTransaction,
}

impl _SuspendedTransactionPool {
    pub fn _insert(&mut self, tx: TransactionReference) {
        assert_eq!(
            self.suspended_tx_pool.insert((tx.sender_address, tx.nonce), tx),
            None,
            "Keys should be unique; duplicates are checked prior."
        );
    }
}
