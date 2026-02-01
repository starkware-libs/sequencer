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
    },
}

pub trait TransactionQueueTrait: Send + Sync {
    fn insert(&mut self, tx_reference: TransactionReference, validate_resource_bounds: bool);

    fn pop_ready_chunk(&mut self, n_txs: usize) -> Vec<TransactionReference>;

    fn remove_by_address(&mut self, address: ContractAddress) -> bool;

    fn remove_by_hash(&mut self, tx_hash: TransactionHash) -> bool;

    fn remove_txs(&mut self, txs: &[TransactionReference]) -> Vec<TransactionReference>;

    // TODO(Ayelet): Rethink this method after implementing FIFO queue, since it doesn't use
    // get_nonce.
    fn get_nonce(&self, address: ContractAddress) -> Option<Nonce>;

    fn has_ready_txs(&self) -> bool;

    fn update_gas_price_threshold(&mut self, threshold: GasPrice);

    fn iter_over_ready_txs(&self) -> Box<dyn Iterator<Item = &TransactionReference> + '_>;

    fn queue_snapshot(&self) -> TransactionQueueSnapshot;

    fn rewind_txs(&mut self, rewind_data: RewindData) -> HashSet<TransactionHash>;

    // TODO(Ayelet): Rethink these methods after implementing FIFO queue, since it doesn't have a
    // concept of "pending" transactions.
    fn priority_queue_len(&self) -> usize;

    fn pending_queue_len(&self) -> usize;

    #[cfg(test)]
    fn pending_txs(&self) -> Vec<TransactionReference>;
}
