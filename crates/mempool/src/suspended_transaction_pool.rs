use std::collections::HashMap;

use starknet_api::core::{ContractAddress, Nonce};

use crate::mempool::TransactionReference;

type AddressNonceToTransaction = HashMap<(ContractAddress, Nonce), TransactionReference>;

#[derive(Debug, Default)]
pub struct SuspendedTransactionPool {
    _suspended_tx_pool: AddressNonceToTransaction,
}

impl SuspendedTransactionPool {
    // TODO(Ayelet): Implement this function.
    pub fn remove_up_to_nonce_and_sequential(&mut self, _address: ContractAddress, _nonce: Nonce) {}

    // TODO(Ayelet): Implement this function.
    pub fn contains(&self, _address: ContractAddress, _nonce: Nonce) -> bool {
        false
    }
}
