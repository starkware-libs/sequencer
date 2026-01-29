use std::collections::{HashMap, HashSet};

use apollo_mempool_types::mempool_types::TransactionQueueSnapshot;
use starknet_api::block::GasPrice;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::transaction::TransactionHash;

use crate::mempool::TransactionReference;

pub trait TransactionQueueTrait: Send + Sync {
    fn insert(&mut self, tx_reference: TransactionReference, validate_resource_bounds: bool);

    fn pop_ready_chunk(&mut self, n_txs: usize) -> Vec<TransactionReference>;

    fn remove_by_address(&mut self, address: ContractAddress) -> bool;

    fn remove_txs(&mut self, txs: &[TransactionReference]) -> Vec<TransactionReference>;

    fn get_nonce(&self, address: ContractAddress) -> Option<Nonce>;

    fn has_ready_txs(&self) -> bool;

    fn update_gas_price_threshold(&mut self, threshold: GasPrice);

    fn iter_over_ready_txs(&self) -> Box<dyn Iterator<Item = &TransactionReference> + '_>;

    fn queue_snapshot(&self) -> TransactionQueueSnapshot;

    fn rewind_txs(
        &mut self,
        next_txs_by_address: HashMap<ContractAddress, TransactionReference>,
    ) -> HashSet<TransactionHash>;

    // TODO(Ayelet): Rethink these methods after implementing FIFO queue, since it doesn't have a
    // concept of "pending" transactions.
    fn priority_queue_len(&self) -> usize;

    fn pending_queue_len(&self) -> usize;

    #[cfg(test)]
    fn pending_txs(&self) -> Vec<TransactionReference>;
}
