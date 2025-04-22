use std::collections::HashSet;

use apollo_l1_provider_types::{InvalidValidationStatus, ValidationStatus};
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::transaction::TransactionHash;

use crate::soft_delete_index_map::SoftDeleteIndexMap;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransactionManager {
    pub uncommited: SoftDeleteIndexMap,
    pub committed: HashSet<TransactionHash>,
    pub rejected: SoftDeleteIndexMap,
}

impl TransactionManager {
    pub fn start_block(&mut self) {
        self.uncommited.rollback_staging();
        self.rejected.rollback_staging();
    }

    pub fn get_txs(&mut self, n_txs: usize) -> Vec<L1HandlerTransaction> {
        let mut txs = Vec::with_capacity(n_txs);

        for _ in 0..n_txs {
            match self.uncommited.soft_pop_front().cloned() {
                Some(tx) => txs.push(tx),
                None => break,
            }
        }
        txs
    }

    pub fn validate_tx(&mut self, tx_hash: TransactionHash) -> ValidationStatus {
        if self.committed.contains(&tx_hash) {
            return ValidationStatus::Invalid(InvalidValidationStatus::AlreadyIncludedOnL2);
        }

        if self.uncommited.soft_remove(tx_hash).is_some() {
            ValidationStatus::Validated
        } else if self.uncommited.is_staged(&tx_hash) {
            ValidationStatus::Invalid(InvalidValidationStatus::AlreadyIncludedInProposedBlock)
        } else {
            ValidationStatus::Invalid(InvalidValidationStatus::ConsumedOnL1OrUnknown)
        }
    }

    pub fn commit_txs(&mut self, committed_txs: &[TransactionHash]) {
        // Committed L1 transactions are dropped here, do we need to them for anything?
        self.uncommited.commit(committed_txs);
        // Add all committed tx hashes to the committed buffer, regardless of if they're known or
        // not, in case we haven't scraped them yet and another node did.
        self.committed.extend(committed_txs)
    }

    /// Adds a transaction to the transaction manager, return false iff the transaction already
    /// existed.
    pub fn add_tx(&mut self, tx: L1HandlerTransaction) -> bool {
        self.committed.contains(&tx.tx_hash) || self.uncommited.insert(tx)
    }

    pub fn committed_includes(&self, tx_hashes: &[TransactionHash]) -> bool {
        tx_hashes.iter().all(|tx| self.committed.contains(tx))
    }

    pub fn store_rejected_txs(&mut self, tx_hashes: &[TransactionHash]) {
        for tx_hash in tx_hashes {
            if let Some(tx) = self.uncommited.get_transaction(tx_hash) {
                self.rejected.insert(tx.clone());
            }
        }
    }
}
