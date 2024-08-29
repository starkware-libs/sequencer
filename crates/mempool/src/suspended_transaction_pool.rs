use std::collections::HashMap;

use starknet_api::core::{ContractAddress, Nonce};

use crate::mempool::TransactionReference;

type AddressNonceToTransaction = HashMap<(ContractAddress, Nonce), TransactionReference>;

#[derive(Debug, Default)]
pub struct SuspendedTransactionPool {
    _suspended_tx_pool: AddressNonceToTransaction,
}

impl SuspendedTransactionPool {
    pub fn _insert(&mut self, tx: TransactionReference) {
        self._suspended_tx_pool.insert((tx.sender_address, tx.nonce), tx);
    }
}
