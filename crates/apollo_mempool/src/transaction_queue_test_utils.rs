use std::collections::HashMap;

use starknet_api::block::NonzeroGasPrice;

use crate::mempool::TransactionReference;
use crate::transaction_queue::{PendingTransaction, PriorityTransaction, TransactionQueue};

impl TransactionQueue {
    pub fn new(
        priority_queue: Vec<TransactionReference>,
        pending_queue: Vec<TransactionReference>,
        gas_price_threshold: NonzeroGasPrice,
    ) -> Self {
        // Build address to nonce mapping, check queues are mutually exclusive in addresses.
        let tx_references = pending_queue.iter().chain(priority_queue.iter());
        let mut address_to_tx = HashMap::new();
        for tx_ref in tx_references {
            let address = tx_ref.address;
            if address_to_tx.insert(address, *tx_ref).is_some() {
                panic!("Duplicate address: {address}; queues must be mutually exclusive.");
            }
        }

        TransactionQueue {
            priority_queue: priority_queue.into_iter().map(PriorityTransaction).collect(),
            pending_queue: pending_queue.into_iter().map(PendingTransaction).collect(),
            address_to_tx,
            gas_price_threshold,
        }
    }

    pub fn pending_txs(&self) -> Vec<TransactionReference> {
        self.pending_queue.iter().rev().map(|tx| tx.0).collect()
    }
}
