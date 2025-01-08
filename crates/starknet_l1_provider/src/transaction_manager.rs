use indexmap::IndexMap;
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_l1_provider_types::ValidationStatus;

use crate::soft_delete_index_map::SoftDeleteIndexMap;

#[derive(Debug, Default)]
pub struct TransactionManager {
    pub txs: SoftDeleteIndexMap,
    pub committed: IndexMap<TransactionHash, Option<L1HandlerTransaction>>,
}

impl TransactionManager {
    pub fn get_txs(&mut self, n_txs: usize) -> Vec<L1HandlerTransaction> {
        let mut txs = Vec::with_capacity(n_txs);

        for _ in 0..n_txs {
            match self.txs.soft_pop_front().cloned() {
                Some(tx) => txs.push(tx),
                None => break,
            }
        }
        txs
    }

    pub fn validate_tx(&mut self, tx_hash: TransactionHash) -> ValidationStatus {
        if self.committed.contains_key(&tx_hash) {
            return ValidationStatus::AlreadyIncludedOnL2;
        }

        if self.txs.soft_remove(tx_hash).is_some() {
            ValidationStatus::Validated
        } else if self.txs.is_staged(&tx_hash) {
            ValidationStatus::AlreadyIncludedInPropsedBlock
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
