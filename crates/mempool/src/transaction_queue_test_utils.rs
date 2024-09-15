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
    pub fn _assert_eq_priority_and_pending_content(&self, tx_queue: &TransactionQueue) {
        assert_eq!(self.priority_queue.as_ref().unwrap(), tx_queue.priority_queue);
        assert_eq!(self.pending_queue.as_ref().unwrap(), tx_queue.pending_queue);
        assert_eq!(self.address_to_tx.as_ref().unwrap(), tx_queue.address_to_tx);
    }
}

#[derive(Debug, Default)]
struct TransactionQueueContentBuilder {
    priority_queue: Option<BTreeSet<PriorityTransaction>>,
    pending_queue: Option<BTreeSet<PendingTransaction>>,
    address_to_tx: Option<HashMap<ContractAddress, TransactionReference>>,
}

impl TransactionQueueContentBuilder {
    fn new() -> Self {
        Self::default()
    }

    fn with_priority<P>(mut self, priority_txs: P) -> Self
    where
        P: IntoIterator<Item = TransactionReference>,
    {
        self.priority_queue = Some(priority_txs.into_iter().map(Into::into));
        self.address_to_tx.get_or_insert_with(HashMap::new);

        self.address_to_tx
            .as_mut()
            .unwrap()
            .extend(priority_txs.iter().map(|tx| (tx.sender_address, tx)));

        self
    }

    fn with_pending<P>(mut self, pending_txs: P) -> Self
    where
        P: IntoIterator<Item = TransactionReference>,
    {
        self.priority_queue = Some(pending_txs.into_iter().map(Into::into));
        self.address_to_tx.get_or_insert_with(HashMap::new);

        self.address_to_tx
            .as_mut()
            .unwrap()
            .extend(pending_txs.iter().map(|tx| (tx.sender_address, tx)));

        self
    }

    fn build(self) -> MempoolContent {
        MempoolContent {
            priority_queue: self.priority_queue,
            pending_queue: self.pending_queue,
            address_to_tx: self.address_to_tx,
        }
    }
}

impl From<TransactionQueueContent> for TransactionQueue {
    fn from(tx_queue_content: TransactionQueueContent) -> TransactionQueue {
        let TransactionQueueContent { priority_queue, pending_queue, address_to_tx } =
            tx_queue_content;
        TransactionQueue { priority_queue, pending_queue, address_to_tx, ..Default::default() }
    }
}

impl From<TransactionQueue> for TransactionQueueContent {
    fn from(tx_queue: TransactionQueue) -> TransactionQueueContent {
        let TransactionQueue { priority_queue, pending_queue, address_to_tx, .. } = tx_queue;
        TransactionQueueContent {
            priority_queue: Some(priority_queue),
            pending_queue: Some(pending_queue),
            address_to_tx: Some(address_to_tx),
        }
    }
}
