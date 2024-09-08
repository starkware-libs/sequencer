use std::collections::HashMap;

use starknet_api::core::{ContractAddress, Nonce};

use crate::eviction_manager::_Evictable;
use crate::mempool::TransactionReference;

type AddressNonceToTransaction = HashMap<(ContractAddress, Nonce), TransactionReference>;

#[derive(Debug, Default)]
pub struct SuspendedTransactionPool {
    _suspended_tx_pool: AddressNonceToTransaction,
}

impl _Evictable for SuspendedTransactionPool {
    fn _insert(&mut self, _tx: TransactionReference) -> bool {
        todo!()
    }

    fn _suggest_tx_to_evict(&self) -> Option<TransactionReference> {
        todo!()
    }

    fn _align_with_current_state(&mut self, _address: ContractAddress, _nonce: Nonce) {
        todo!()
    }
}
