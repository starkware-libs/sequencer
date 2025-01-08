use indexmap::{IndexMap, IndexSet};
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_l1_provider_types::ValidationStatus;

#[derive(Debug, Default)]
pub struct TransactionManager {
    pub txs: IndexMap<TransactionHash, L1HandlerTransaction>,
    pub proposed_txs: IndexSet<TransactionHash>,
    pub on_l2_awaiting_l1_consumption: IndexSet<TransactionHash>,
}

impl TransactionManager {
    pub fn get_txs(&mut self, n_txs: usize) -> Vec<L1HandlerTransaction> {
        let (tx_hashes, txs): (Vec<_>, Vec<_>) = self
            .txs
            .iter()
            .skip(self.proposed_txs.len()) // Transactions are proposed FIFO.
            .take(n_txs)
            .map(|(&hash, tx)| (hash, tx.clone()))
            .unzip();

        self.proposed_txs.extend(tx_hashes);
        txs
    }

    pub fn tx_status(&self, tx_hash: TransactionHash) -> ValidationStatus {
        if self.txs.contains_key(&tx_hash) {
            ValidationStatus::Validated
        } else if self.on_l2_awaiting_l1_consumption.contains(&tx_hash) {
            ValidationStatus::AlreadyIncludedOnL2
        } else {
            ValidationStatus::ConsumedOnL1OrUnknown
        }
    }

    pub fn _add_unconsumed_l1_not_in_l2_block_tx(&mut self, _tx: L1HandlerTransaction) {
        todo!(
            "Check if tx is in L2, if it isn't on L2 add it to the txs buffer, otherwise print
             debug and do nothing."
        )
    }

    pub fn _mark_tx_included_on_l2(&mut self, _tx_hash: &TransactionHash) {
        todo!("Adds the tx hash to l2 buffer; remove tx from the txs storage if it's there.")
    }
}
