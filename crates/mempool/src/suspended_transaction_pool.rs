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
}
