use std::collections::{BTreeSet, HashMap};

use pretty_assertions::assert_eq;
use starknet_api::block::GasPrice;

use crate::mempool::TransactionReference;
use crate::transaction_queue::{PendingTransaction, PriorityTransaction, TransactionQueue};

/// Represents the internal content of the transaction queue.
/// Enables customized (and potentially inconsistent) creation for unit testing.
/// Note: gas price threshold is only used for building the (non-test) queue struct.
#[derive(Debug, Default)]
pub struct TransactionQueueContent {
    priority_queue: Option<BTreeSet<PriorityTransaction>>,
    pending_queue: Option<BTreeSet<PendingTransaction>>,
    gas_price_threshold: Option<GasPrice>,
}

impl TransactionQueueContent {
    #[track_caller]
    pub fn assert_eq(&self, tx_queue: &TransactionQueue) {
        if let Some(priority_queue) = &self.priority_queue {
            assert_eq!(&tx_queue.priority_queue, priority_queue);
        }

        if let Some(pending_queue) = &self.pending_queue {
            assert_eq!(&tx_queue.pending_queue, pending_queue);
        }
    }

    pub fn complete_to_tx_queue(self) -> TransactionQueue {
        let pending_queue = self.pending_queue.unwrap_or_default();
        let priority_queue = self.priority_queue.unwrap_or_default();
        let gas_price_threshold = self.gas_price_threshold.unwrap_or_default();

        // Build address to nonce mapping, check queues are mutually exclusive in addresses.
        let tx_references = pending_queue
            .iter()
            .map(|pending_tx| pending_tx.0)
            .chain(priority_queue.iter().map(|priority_tx| priority_tx.0));
        let mut address_to_tx = HashMap::new();
        for tx_ref in tx_references {
            let address = tx_ref.address;
            if address_to_tx.insert(address, tx_ref).is_some() {
                panic!("Duplicate address: {address}; queues must be mutually exclusive.");
            }
        }

        TransactionQueue { priority_queue, pending_queue, address_to_tx, gas_price_threshold }
    }
}

#[derive(Debug, Default)]
pub struct TransactionQueueContentBuilder {
    priority_queue: Option<BTreeSet<PriorityTransaction>>,
    pending_queue: Option<BTreeSet<PendingTransaction>>,
    gas_price_threshold: Option<GasPrice>,
}

impl TransactionQueueContentBuilder {
    pub fn with_priority<P>(mut self, priority_txs: P) -> Self
    where
        P: IntoIterator<Item = TransactionReference>,
    {
        self.priority_queue =
            Some(priority_txs.into_iter().map(PriorityTransaction::from).collect());
        self
    }

    pub fn _with_pending<P>(mut self, pending_txs: P) -> Self
    where
        P: IntoIterator<Item = TransactionReference>,
    {
        self.pending_queue = Some(pending_txs.into_iter().map(PendingTransaction::from).collect());
        self
    }

    pub fn _with_gas_price_threshold(mut self, gas_price_threshold: u128) -> Self {
        self.gas_price_threshold = Some(gas_price_threshold.into());
        self
    }

    pub fn build(self) -> Option<TransactionQueueContent> {
        if self.is_default() {
            return None;
        }

        Some(TransactionQueueContent {
            priority_queue: self.priority_queue,
            pending_queue: self.pending_queue,
            gas_price_threshold: self.gas_price_threshold,
        })
    }

    fn is_default(&self) -> bool {
        self.priority_queue.is_none()
            && self.pending_queue.is_none()
            && self.gas_price_threshold.is_none()
    }
}
