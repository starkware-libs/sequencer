use std::collections::HashMap;

use apollo_mempool_types::mempool_types::TransactionQueueSnapshot;
use starknet_api::block::GasPrice;
use starknet_api::core::{ContractAddress, Nonce};

use crate::mempool::TransactionReference;

pub trait TransactionQueueTrait: Send + Sync {
    fn insert(&mut self, tx_reference: TransactionReference, validate_resource_bounds: bool);

    fn pop_ready_chunk(&mut self, n_txs: usize) -> Vec<TransactionReference>;

    fn remove_by_address(&mut self, address: ContractAddress) -> bool;

    fn remove_txs(&mut self, txs: &[TransactionReference]) -> Vec<TransactionReference>;

    // Default implementation returns None (for queues that don't track nonces per address).
    fn get_nonce(&self, _address: ContractAddress) -> Option<Nonce> {
        None
    }

    fn has_ready_txs(&self) -> bool;

    // Default implementation is a no-op (for queues that don't use gas price thresholds).
    fn update_gas_price_threshold(&mut self, _threshold: GasPrice) {}

    fn iter_over_ready_txs(&self) -> impl Iterator<Item = &TransactionReference>;

    fn queue_snapshot(&self) -> TransactionQueueSnapshot;

    fn rewind_txs(
        &mut self,
        next_txs_by_address: HashMap<ContractAddress, TransactionReference>,
        validate_resource_bounds: bool,
    );

    // Default implementation returns 0 (for queues that don't distinguish priority/pending).
    fn priority_queue_len(&self) -> usize {
        0
    }

    // Default implementation returns 0 (for queues that don't distinguish priority/pending).
    fn pending_queue_len(&self) -> usize {
        0
    }
}
