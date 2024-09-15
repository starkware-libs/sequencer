use std::collections::HashMap;

use starknet_api::core::{ContractAddress, Nonce};

use crate::mempool::TransactionReference;

type AddressNonceToTransaction = HashMap<(ContractAddress, Nonce), TransactionReference>;

#[derive(Debug, Default)]
pub struct SuspendedTransactionPool {
    _suspended_tx_pool: AddressNonceToTransaction,
}

impl SuspendedTransactionPool {
    pub fn _remove(&mut self, tx: &TransactionReference) -> bool {
        self._suspended_tx_pool.remove(&(tx.sender_address, tx.nonce)).is_some()
    }
}
