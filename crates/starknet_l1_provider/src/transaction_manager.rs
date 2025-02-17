use indexmap::IndexMap;
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_l1_provider_types::ValidationStatus;

use crate::soft_delete_index_map::SoftDeleteIndexMap;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransactionManager {
    pub txs: SoftDeleteIndexMap,
    pub committed: IndexMap<TransactionHash, Option<L1HandlerTransaction>>,
}

impl TransactionManager {
    pub fn start_block(&mut self) {
        self.txs.rollback_staging();
    }

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
            ValidationStatus::AlreadyIncludedInProposedBlock
        } else {
            ValidationStatus::ConsumedOnL1OrUnknown
        }
    }

    pub fn commit_txs(&mut self, committed_txs: &[TransactionHash]) {
        self.txs.commit(committed_txs);
    }

    /// Adds a transaction to the transaction manager, return false iff the transaction already
    /// existed.
    pub fn add_tx(&mut self, tx: L1HandlerTransaction) -> bool {
        self.committed.contains_key(&tx.tx_hash) || self.txs.insert(tx)
    }
}
