use std::collections::HashMap;

use apollo_mempool_types::mempool_types::{TransactionQueueSnapshot, TxBlockMetadata};
use indexmap::IndexSet;
use starknet_api::block::{BlockNumber, GasPrice, UnixTimestamp};
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::transaction::TransactionHash;

use crate::mempool::TransactionReference;

pub(crate) struct BlockMetadata {
    pub timestamp: UnixTimestamp,
    pub block_number: BlockNumber,
}

// Data needed for rewinding transactions back into the queue after a block commit.
// Different queue types require different data.
pub enum RewindData<'a> {
    // Data for fee-priority queue rewind.
    FeePriority {
        // Map of next transaction to rewind for each address (from tx_pool).
        next_txs_by_address: &'a HashMap<ContractAddress, TransactionReference>,
        // Whether to validate resource bounds on insertion.
        validate_resource_bounds: bool,
    },
    // Data for FIFO queue rewind.
    Fifo {
        // Map of committed nonces by address.
        committed_nonces: &'a HashMap<ContractAddress, Nonce>,
        // Set of rejected transaction hashes.
        rejected_tx_hashes: &'a IndexSet<TransactionHash>,
    },
}

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

    fn iter_over_ready_txs(&self) -> Box<dyn Iterator<Item = &TransactionReference> + '_>;

    fn queue_snapshot(&self) -> TransactionQueueSnapshot;

    /// Rewinds transactions back into the queue after a block commit.
    /// Returns the set of transaction hashes that were rewound (for tracking purposes).
    fn rewind_txs(&mut self, rewind_data: RewindData<'_>) -> IndexSet<TransactionHash>;

    // Default implementation returns 0 (for queues that don't distinguish priority/pending).
    fn priority_queue_len(&self) -> usize {
        0
    }

    // Default implementation returns 0 (for queues that don't distinguish priority/pending).
    fn pending_queue_len(&self) -> usize {
        0
    }

    // Default implementation is a no-op (for queues that don't support metadata updates).
    fn update_tx_block_metadata(&mut self, _tx_hash: TransactionHash, _metadata: TxBlockMetadata) {}

    // Returns the block metadata and may update queue-internal state.
    // Returns `None` for queues that don't use block metadata (fee-priority mode).
    fn resolve_metadata(&mut self) -> Option<BlockMetadata> {
        None
    }

    // Default implementation returns empty vec.
    #[cfg(test)]
    fn pending_txs(&self) -> Vec<TransactionReference> {
        Vec::new()
    }
}
