use std::collections::{BTreeSet, HashMap};

use pretty_assertions::assert_eq;

use crate::mempool::TransactionReference;
use crate::transaction_queue::{
    AddressToTransactionReference,
    PendingTransaction,
    PriorityTransaction,
    TransactionQueue,
};

/// Represents the internal content of the transaction queue.
/// Enables customized (and potentially inconsistent) creation for unit testing.
/// The `gas_price_threshold` is used for builing the struct, but it is not checked. This is because
/// it's an input parameter for the Mempool, and no logic within the Mempool modifies its value.
#[derive(Debug, Default)]
pub struct TransactionQueueContent {
    priority_queue: Option<BTreeSet<PriorityTransaction>>,
    pending_queue: Option<BTreeSet<PendingTransaction>>,
    address_to_tx: Option<AddressToTransactionReference>,
    gas_price_threshold: Option<u128>,
}

impl TransactionQueueContent {
    pub fn _assert_eq_priority_queue_and_pending_queue_content(&self, tx_queue: &TransactionQueue) {
        assert_eq!(self.priority_queue.as_ref().unwrap(), &tx_queue.priority_queue);
        assert_eq!(self.pending_queue.as_ref().unwrap(), &tx_queue.pending_queue);
        assert_eq!(self.address_to_tx.as_ref().unwrap(), &tx_queue.address_to_tx);
    }

    pub fn _assert_eq_priority_queue_content(&self, tx_queue: &TransactionQueue) {
        assert_eq!(self.priority_queue.as_ref().unwrap(), &tx_queue.priority_queue);
        assert_eq!(self.address_to_tx.as_ref().unwrap(), &tx_queue.address_to_tx);
    }

    pub fn _assert_eq_pending_queue_content(&self, tx_queue: &TransactionQueue) {
        assert_eq!(self.pending_queue.as_ref().unwrap(), &tx_queue.pending_queue);
        assert_eq!(self.address_to_tx.as_ref().unwrap(), &tx_queue.address_to_tx);
    }
}

#[derive(Debug, Default)]
pub struct TransactionQueueContentBuilder {
    _priority_queue: Option<BTreeSet<PriorityTransaction>>,
    _pending_queue: Option<BTreeSet<PendingTransaction>>,
    _address_to_tx: Option<AddressToTransactionReference>,
    _gas_price_threshold: Option<u128>,
}

impl TransactionQueueContentBuilder {
    pub fn _with_priority<P>(mut self, priority_txs: P) -> Self
    where
        P: IntoIterator<Item = TransactionReference>,
    {
        let priority_txs: Vec<TransactionReference> = priority_txs.into_iter().collect();

        self._address_to_tx
            .get_or_insert_with(HashMap::new)
            .extend(priority_txs.iter().map(|tx| (tx.sender_address, *tx)));
        self._priority_queue =
            Some(priority_txs.into_iter().map(PriorityTransaction::from).collect());

        self
    }

    pub fn _with_pending<P>(mut self, pending_txs: P) -> Self
    where
        P: IntoIterator<Item = TransactionReference>,
    {
        let pending_txs_vec: Vec<TransactionReference> = pending_txs.into_iter().collect();

        self._address_to_tx
            .get_or_insert_with(HashMap::new)
            .extend(pending_txs_vec.iter().map(|tx| (tx.sender_address, *tx)));
        self._pending_queue =
            Some(pending_txs_vec.into_iter().map(PendingTransaction::from).collect());

        self
    }

    pub fn _with_gas_price_threshold(mut self, gas_price_threshold: u128) -> Self {
        self._gas_price_threshold = Some(gas_price_threshold);
        self
    }

    pub fn _build(self) -> TransactionQueueContent {
        TransactionQueueContent {
            priority_queue: self._priority_queue,
            pending_queue: self._pending_queue,
            address_to_tx: self._address_to_tx,
            gas_price_threshold: self._gas_price_threshold,
        }
    }
}

impl From<TransactionQueueContent> for TransactionQueue {
    fn from(tx_queue_content: TransactionQueueContent) -> Self {
        let TransactionQueueContent {
            priority_queue,
            pending_queue,
            address_to_tx,
            gas_price_threshold,
        } = tx_queue_content;
        TransactionQueue {
            priority_queue: priority_queue.unwrap_or_default(),
            pending_queue: pending_queue.unwrap_or_default(),
            address_to_tx: address_to_tx.unwrap_or_default(),
            gas_price_threshold: gas_price_threshold.unwrap_or_default(),
        }
    }
}
