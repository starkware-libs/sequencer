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
}

impl TransactionQueueContent {
    pub fn _assert_eq_priority_and_pending_queues(&self, tx_queue: &TransactionQueue) {
        self._assert_eq_priority_queue(tx_queue);
        self._assert_eq_pending_queue(tx_queue);
    }

    pub fn _assert_eq_priority_queue(&self, tx_queue: &TransactionQueue) {
        assert_eq!(self.pending_queue.as_ref().unwrap(), &tx_queue.pending_queue);
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
}

impl TransactionQueueContentBuilder {
    pub fn _new() -> Self {
        Self::default()
    }

    pub fn _with_priority<P>(mut self, priority_txs: P) -> Self
    where
        P: IntoIterator<Item = TransactionReference> + Copy,
    {
        self._priority_queue = Some(priority_txs.into_iter().map(Into::into).collect());
        self._address_to_tx.get_or_insert_with(HashMap::new);

        self._address_to_tx
            .as_mut()
            .unwrap()
            .extend(priority_txs.into_iter().map(|tx| (tx.sender_address, tx)));

        self
    }

    pub fn _with_pending<P>(mut self, pending_txs: P) -> Self
    where
        P: IntoIterator<Item = TransactionReference> + Copy,
    {
        self._pending_queue = Some(pending_txs.into_iter().map(Into::into).collect());
        self._address_to_tx.get_or_insert_with(HashMap::new);

        self._address_to_tx
            .as_mut()
            .unwrap()
            .extend(pending_txs.into_iter().map(|tx| (tx.sender_address, tx)));

        self
    }

    pub fn _build(self) -> TransactionQueueContent {
        TransactionQueueContent {
            priority_queue: self._priority_queue,
            pending_queue: self._pending_queue,
            address_to_tx: self._address_to_tx,
        }
    }
}

impl From<TransactionQueueContent> for TransactionQueue {
    fn from(tx_queue_content: TransactionQueueContent) -> Self {
        let TransactionQueueContent { priority_queue, pending_queue, address_to_tx } =
            tx_queue_content;
        TransactionQueue {
            priority_queue: priority_queue.unwrap(),
            pending_queue: pending_queue.unwrap(),
            address_to_tx: address_to_tx.unwrap(),
            ..Default::default()
        }
    }
}
