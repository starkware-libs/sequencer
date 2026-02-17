use std::collections::{HashMap, VecDeque};

use apollo_mempool_types::mempool_types::TransactionQueueSnapshot;
use starknet_api::block::GasPrice;
use starknet_api::core::{ContractAddress, Nonce};

use crate::mempool::TransactionReference;
use crate::transaction_queue_trait::TransactionQueueTrait;

/// A FIFO (First-In-First-Out) transaction queue that orders transactions by arrival time.
/// Used in Echonet mode to preserve the original transaction order from the source chain.
#[derive(Debug, Default)]
pub struct FifoTransactionQueue {
    // Queue of transactions ordered by arrival time (FIFO).
    queue: VecDeque<TransactionReference>,
    // Map from address to transaction for efficient lookups.
    address_to_tx: HashMap<ContractAddress, TransactionReference>,
}

impl TransactionQueueTrait for FifoTransactionQueue {
    fn insert(&mut self, tx_reference: TransactionReference, _validate_resource_bounds: bool) {
        self.address_to_tx.insert(tx_reference.address, tx_reference);
        self.queue.push_back(tx_reference);
    }

    fn pop_ready_chunk(&mut self, n_txs: usize) -> Vec<TransactionReference> {
        let txs: Vec<TransactionReference> = (0..n_txs)
            .filter_map(|_| {
                self.queue.pop_front().inspect(|tx| {
                    self.address_to_tx.remove(&tx.address);
                })
            })
            .collect();
        txs
    }

    fn remove_by_address(&mut self, address: ContractAddress) -> bool {
        if self.address_to_tx.remove(&address).is_none() {
            return false;
        }

        // Remove from queue
        self.queue.retain(|tx| tx.address != address);
        true
    }

    fn remove_txs(&mut self, txs: &[TransactionReference]) -> Vec<TransactionReference> {
        let mut removed_txs = Vec::new();
        for tx in txs {
            let queued_tx = self.address_to_tx.get(&tx.address);
            if queued_tx.is_some_and(|queued_tx| queued_tx.tx_hash == tx.tx_hash) {
                self.remove_by_address(tx.address);
                removed_txs.push(*tx);
            }
        }
        removed_txs
    }

    fn get_nonce(&self, address: ContractAddress) -> Option<Nonce> {
        self.address_to_tx.get(&address).map(|tx| tx.nonce)
    }

    fn has_ready_txs(&self) -> bool {
        !self.queue.is_empty()
    }

    fn iter_over_ready_txs(&self) -> Box<dyn Iterator<Item = &TransactionReference> + '_> {
        Box::new(self.queue.iter())
    }

    fn queue_snapshot(&self) -> TransactionQueueSnapshot {
        let priority_queue = self.queue.iter().map(|tx| tx.tx_hash).collect();
        let pending_queue = Vec::new();

        TransactionQueueSnapshot { gas_price_threshold: GasPrice(0), priority_queue, pending_queue }
    }

    fn rewind_txs(
        &mut self,
        next_txs_by_address: HashMap<ContractAddress, TransactionReference>,
        _validate_resource_bounds: bool,
    ) {
        // Rewind by re-inserting the next transaction for each address.
        for (address, tx_reference) in next_txs_by_address {
            self.remove_by_address(address);
            self.insert(tx_reference, false);
        }
    }

    fn priority_queue_len(&self) -> usize {
        self.queue.len()
    }

    fn pending_queue_len(&self) -> usize {
        0
    }
}
