use apollo_mempool_types::mempool_types::TransactionQueueSnapshot;
use starknet_api::block::GasPrice;
use starknet_api::core::{ContractAddress, Nonce};

use crate::mempool::TransactionReference;

/// Trait for transaction queue implementations.
/// Abstracts the differences between priority-based and FIFO queues.
pub trait TransactionQueueTrait: Send + Sync {
    /// Insert a transaction into the queue.
    fn insert(&mut self, tx_reference: TransactionReference, validate_resource_bounds: bool);

    /// Pop up to `n_txs` transactions from the queue (in queue's order).
    /// For priority queue: use `pop_ready_chunk`.
    /// For FIFO queue: use `pop`.
    fn pop_ready_chunk(&mut self, n_txs: usize) -> Vec<TransactionReference>;

    /// Remove transaction(s) for the given address.
    /// For priority queue: use `remove`.
    /// For FIFO queue: use `remove_by_address`.
    fn remove(&mut self, address: ContractAddress) -> bool;

    /// Remove the given transactions from the queue.
    /// Returns the transactions that were actually removed.
    fn remove_txs(&mut self, txs: &[TransactionReference]) -> Vec<TransactionReference>;

    /// Get the nonce of the transaction for the given address (if any).
    fn get_nonce(&self, address: ContractAddress) -> Option<Nonce>;

    /// Check if queue has ready transactions.
    fn has_ready_txs(&self) -> bool;

    /// Update gas price threshold (no-op for FIFO queue).
    fn update_gas_price_threshold(&mut self, threshold: GasPrice);

    /// Iterate over ready transactions.
    /// For priority queue: use `iter_over_ready_txs`.
    /// For FIFO queue: use `iter_ready_txs`.
    fn iter_over_ready_txs(&self) -> impl Iterator<Item = &TransactionReference> + '_;

    /// Get a snapshot of the queue state (for monitoring/debugging).
    fn queue_snapshot(&self) -> TransactionQueueSnapshot;
}
