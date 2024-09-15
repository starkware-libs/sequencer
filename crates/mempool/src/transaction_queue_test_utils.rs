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
    priority_queue: BTreeSet<PriorityTransaction>,
    pending_queue: BTreeSet<PendingTransaction>,
    address_to_tx: HashMap<ContractAddress, TransactionReference>,
}

impl TransactionQueueContent {
    pub fn _with_priority_and_pending<Q, P>(priority_txs: Q, pending_txs: P) -> Self
    where
        Q: IntoIterator<Item = TransactionReference>,
        P: IntoIterator<Item = TransactionReference>,
    {
        let priority_queue: BTreeSet<PriorityTransaction> =
            priority_txs.into_iter().map(Into::into).collect();

        let pending_queue: BTreeSet<PendingTransaction> =
            pending_txs.into_iter().map(Into::into).collect();

        let address_to_tx: HashMap<ContractAddress, TransactionReference> = priority_queue
            .iter()
            .map(|tx| tx.clone().0)
            .chain(pending_queue.iter().map(|tx| tx.clone().0))
            .map(|tx| (tx.sender_address, tx))
            .collect();

        Self { priority_queue, pending_queue, address_to_tx }
    }

    pub fn _assert_eq_priority_and_pending_content(&self, tx_queue: &TransactionQueue) {
        assert_eq!(self.priority_queue, tx_queue.priority_queue);
        assert_eq!(self.pending_queue, tx_queue.pending_queue);
    }
}

impl From<TransactionQueueContent> for TransactionQueue {
    fn from(tx_queue_content: TransactionQueueContent) -> TransactionQueue {
        let TransactionQueueContent { priority_queue, pending_queue, address_to_tx } =
            tx_queue_content;
        TransactionQueue { priority_queue, pending_queue, address_to_tx, ..Default::default() }
    }
}

impl TransactionQueue {
    fn _from_iter<T: IntoIterator<Item = TransactionReference>>(
        priority_txs: T,
        pending_txs: T,
    ) -> Self {
        let mut tx_queue = Self::default();
        for tx in tx_queue
            .priority_queue
            .iter()
            .map(|tx| tx.0.clone())
            .chain(tx_queue.pending_queue.iter().map(|tx| tx.0.clone()))
        {
            tx_queue.address_to_tx.insert(tx.sender_address, tx);
        }
        for tx in priority_txs {
            tx_queue.priority_queue.insert(tx.into());
        }
        for tx in pending_txs {
            tx_queue.pending_queue.insert(tx.into());
        }

        tx_queue
    }
}
