use std::collections::{BTreeSet, HashMap};

use pretty_assertions::assert_eq;

use crate::mempool::TransactionReference;
use crate::transaction_queue::{PendingTransaction, PriorityTransaction, TransactionQueue};

/// Represents the internal content of the transaction queue.
/// Enables customized (and potentially inconsistent) creation for unit testing.
/// Note: gas price threshold is only used for building the (non-test) queue struct.
#[derive(Debug, Default)]
pub struct _TransactionQueueContent {
    _priority_queue: Option<BTreeSet<PriorityTransaction>>,
    _pending_queue: Option<BTreeSet<PendingTransaction>>,
    _gas_price_threshold: Option<u128>,
}

impl _TransactionQueueContent {
    pub fn _assert_eq_priority_queue_content(&self, tx_queue: &TransactionQueue) {
        assert_eq!(self._priority_queue.as_ref().unwrap(), &tx_queue.priority_queue);
    }

    pub fn _assert_eq_pending_queue_content(&self, tx_queue: &TransactionQueue) {
        assert_eq!(self._pending_queue.as_ref().unwrap(), &tx_queue.pending_queue);
    }

    pub fn _complete_to_tx_queue(self) -> TransactionQueue {
        let pending_queue = self._pending_queue.unwrap_or_default();
        let priority_queue = self._priority_queue.unwrap_or_default();

        // Build address to nonce mapping, check queues are mutually exclusive in addresses.
        let tx_references = pending_queue
            .iter()
            .map(|pending_tx| pending_tx.0)
            .chain(priority_queue.iter().map(|priotiry_tx| priotiry_tx.0));
        let mut address_to_tx = HashMap::new();
        for tx_ref in tx_references {
            if address_to_tx.insert(tx_ref.sender_address, tx_ref).is_some() {
                panic!("The queues must not have address duplicates");
            }
        }

        TransactionQueue {
            priority_queue,
            pending_queue,
            address_to_tx,
            gas_price_threshold: self._gas_price_threshold.unwrap_or_default(),
        }
    }
}

#[derive(Debug, Default)]
pub struct _TransactionQueueContentBuilder {
    _priority_queue: Option<BTreeSet<PriorityTransaction>>,
    _pending_queue: Option<BTreeSet<PendingTransaction>>,
    _gas_price_threshold: Option<u128>,
}

impl _TransactionQueueContentBuilder {
    pub fn _with_priority<P>(mut self, priority_txs: P) -> Self
    where
        P: IntoIterator<Item = TransactionReference>,
    {
        self._priority_queue =
            Some(priority_txs.into_iter().map(PriorityTransaction::from).collect());
        self
    }

    pub fn _with_pending<P>(mut self, pending_txs: P) -> Self
    where
        P: IntoIterator<Item = TransactionReference>,
    {
        self._pending_queue = Some(pending_txs.into_iter().map(PendingTransaction::from).collect());
        self
    }

    pub fn _with_gas_price_threshold(mut self, gas_price_threshold: u128) -> Self {
        self._gas_price_threshold = Some(gas_price_threshold);
        self
    }

    pub fn _build(self) -> _TransactionQueueContent {
        _TransactionQueueContent {
            _priority_queue: self._priority_queue,
            _pending_queue: self._pending_queue,
            _gas_price_threshold: self._gas_price_threshold,
        }
    }
}
