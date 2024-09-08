use starknet_api::core::{ContractAddress, Nonce};

use crate::mempool::TransactionReference;

pub trait _Evictable {
    fn _insert(&mut self, tx: TransactionReference) -> bool;
    fn _suggest_tx_to_evict(&self) -> Option<TransactionReference>;
    fn _align_with_current_state(&mut self, address: ContractAddress, nonce: Nonce);
}
