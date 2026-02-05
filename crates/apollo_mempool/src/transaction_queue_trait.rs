use std::collections::{HashMap, HashSet};

use apollo_mempool_types::mempool_types::TransactionQueueSnapshot;
use indexmap::IndexSet;
use starknet_api::block::GasPrice;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::transaction::TransactionHash;

use crate::mempool::TransactionReference;

pub enum RewindData {
    Fifo {
        staged_tx_refs: Vec<TransactionReference>,
        committed_nonces: HashMap<ContractAddress, Nonce>,
        rejected_tx_hashes: IndexSet<TransactionHash>,
    },
    FeePriority {
        next_txs_by_address: HashMap<ContractAddress, TransactionReference>,
        validate_resource_bounds: bool,
    },
}

pub trait TransactionQueueTrait: Send + Sync {
    fn insert(&mut self, tx_reference: TransactionReference, validate_resource_bounds: bool);

    fn pop_ready_chunk(&mut self, n_txs: usize) -> Vec<TransactionReference>;

    // Default implementation returns false (for queues that don't support removal by address).
    fn remove_by_address(&mut self, _address: ContractAddress) -> bool {
        false
    }

    fn remove_txs(&mut self, txs: &[TransactionReference]) -> Vec<TransactionReference>;

    // Default implementation returns None (for queues that don't track nonces per address).
    fn get_nonce(&self, _address: ContractAddress) -> Option<Nonce> {
        None
    }

    fn has_ready_txs(&self) -> bool;

    // Default implementation is a no-op (for queues that don't use gas price thresholds).
    fn update_gas_price_threshold(&mut self, _threshold: GasPrice) {}

    fn iter_over_ready_txs(&self) -> Box<dyn Iterator<Item = &TransactionReference> + '_>;

    fn queue_snapshot(&self) -> TransactionQueueSnapshot;

    fn rewind_txs(&mut self, rewind_data: RewindData) -> Option<HashSet<TransactionHash>>;

    // Default implementation returns 0 (for queues that don't distinguish priority/pending).
    fn priority_queue_len(&self) -> usize {
        0
    }

    // Default implementation returns 0 (for queues that don't distinguish priority/pending).
    fn pending_queue_len(&self) -> usize {
        0
    }

    #[cfg(test)]
    // Default implementation returns empty vec (for queues that don't distinguish
    // priority/pending).
    fn pending_txs(&self) -> Vec<TransactionReference> {
        Vec::new()
    }

    // Default implementation returns None (for queues that don't track first tx timestamp).
    fn get_first_tx_timestamp(&self) -> Option<u64> {
        None
    }

    // Default implementation is a no-op (for queues that don't track last returned timestamp).
    fn set_last_returned_timestamp(&mut self, _timestamp: u64) {}

    // Default implementation is a no-op (for queues that don't support timestamp updates).
    fn update_timestamps(&mut self, _mappings: HashMap<TransactionHash, u64>) {}
}
