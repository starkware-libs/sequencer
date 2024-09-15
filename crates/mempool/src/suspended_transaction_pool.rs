use std::collections::HashMap;

use starknet_api::core::{ContractAddress, Nonce};

use crate::mempool::TransactionReference;

type _AddressNonceToTransaction = HashMap<(ContractAddress, Nonce), TransactionReference>;

#[derive(Debug)]
pub struct _SuspendedTransactionPool {
    suspended_tx_pool: _AddressNonceToTransaction,
}

impl _SuspendedTransactionPool {
    pub fn _remove(&mut self, tx: &TransactionReference) -> bool {
        self.suspended_tx_pool.remove(&(tx.sender_address, tx.nonce)).is_some()
    }
}
