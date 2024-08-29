use std::collections::HashMap;

use starknet_api::core::{ContractAddress, Nonce};

use crate::mempool::TransactionReference;

type AddressNonceToTransaction = HashMap<(ContractAddress, Nonce), TransactionReference>;

#[derive(Debug, Default)]
pub struct SuspendedTransactionPool {
    _suspended_tx_pool: AddressNonceToTransaction,
}

impl SuspendedTransactionPool {
    pub fn _contains(&self, address: ContractAddress, nonce: Nonce) -> bool {
        self._suspended_tx_pool.contains_key(&(address, nonce))
    }
}
