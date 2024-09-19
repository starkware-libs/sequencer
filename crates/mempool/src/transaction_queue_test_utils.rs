use std::collections::{BTreeSet, HashMap};

use pretty_assertions::assert_eq;
use starknet_api::core::ContractAddress;

use crate::mempool::TransactionReference;
use crate::transaction_queue::{PendingTransaction, PriorityTransaction, TransactionQueue};

// Utils.

/// Represents the internal content of the transaction queue.
/// Enables customized (and potentially inconsistent) creation for unit testing.
#[derive(Debug, Default)]
pub struct TransactionQueueContent {
    priority_queue: Option<BTreeSet<PriorityTransaction>>,
    pending_queue: Option<BTreeSet<PendingTransaction>>,
    address_to_tx: Option<HashMap<ContractAddress, TransactionReference>>,
    gas_price_threshold: Option<u128>,
}

impl TransactionQueueContent {
    pub fn _assert_eq_priority_and_pending_queues(&self, tx_queue: &TransactionQueue) {
        self.assert_eq_priority_queue(tx_queue);
        self._assert_eq_pending_queue(tx_queue);
    }

    pub fn assert_eq_priority_queue(&self, tx_queue: &TransactionQueue) {
        assert_eq!(self.priority_queue.as_ref().unwrap(), &tx_queue.priority_queue);
        assert_eq!(self.address_to_tx.as_ref().unwrap(), &tx_queue.address_to_tx);
    }

    pub fn _assert_eq_pending_queue(&self, tx_queue: &TransactionQueue) {
        assert_eq!(self.pending_queue.as_ref().unwrap(), &tx_queue.pending_queue);
        assert_eq!(self.address_to_tx.as_ref().unwrap(), &tx_queue.address_to_tx);
    }
}

#[derive(Debug, Default)]
pub struct TransactionQueueContentBuilder {
    _priority_queue: Option<BTreeSet<PriorityTransaction>>,
    _pending_queue: Option<BTreeSet<PendingTransaction>>,
    _address_to_tx: Option<HashMap<ContractAddress, TransactionReference>>,
    _gas_price_threshold: Option<u128>,
}

impl TransactionQueueContentBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_priority<P>(mut self, priority_txs: P) -> Self
    where
        P: IntoIterator<Item = TransactionReference>,
    {
        self._priority_queue = Some(priority_txs.into_iter().map(Into::into).collect());
        self._address_to_tx.get_or_insert_with(HashMap::new).extend(
            self._priority_queue.as_ref().unwrap().iter().map(|tx| (tx.sender_address, tx.0)),
        );

        self
    }

    pub fn _with_pending<P>(mut self, pending_txs: P) -> Self
    where
        P: IntoIterator<Item = TransactionReference>,
    {
        self._pending_queue = Some(pending_txs.into_iter().map(Into::into).collect());
        self._address_to_tx.get_or_insert_with(HashMap::new).extend(
            self._pending_queue.as_ref().unwrap().iter().map(|tx| (tx.sender_address, tx.0)),
        );

        self
    }

    pub fn _with_gas_price_threshold(mut self, gas_price_threshold: u128) -> Self {
        self._gas_price_threshold = Some(gas_price_threshold);
        self
    }

    pub fn build(self) -> TransactionQueueContent {
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
