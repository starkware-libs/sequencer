use std::collections::HashSet;

use apollo_l1_provider_types::{InvalidValidationStatus, ValidationStatus};
use indexmap::IndexMap;
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::transaction::TransactionHash;

use crate::soft_delete_index_map::SoftDeleteIndexMap;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransactionManager {
    pub uncommitted: SoftDeleteIndexMap,
    pub rejected: SoftDeleteIndexMap,
    pub committed: HashSet<TransactionHash>,
}

impl TransactionManager {
    pub fn start_block(&mut self) {
        self.uncommitted.rollback_staging();
        self.rejected.rollback_staging();
    }

    pub fn get_txs(&mut self, n_txs: usize) -> Vec<L1HandlerTransaction> {
        let mut txs = Vec::with_capacity(n_txs);

        for _ in 0..n_txs {
            match self.uncommitted.soft_pop_front().cloned() {
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

        if self.uncommitted.soft_remove(tx_hash).is_some()
            || self.rejected.soft_remove(tx_hash).is_some()
        {
            ValidationStatus::Validated
        } else if self.uncommitted.is_staged(&tx_hash) || self.rejected.is_staged(&tx_hash) {
            ValidationStatus::Invalid(InvalidValidationStatus::AlreadyIncludedInProposedBlock)
        } else {
            ValidationStatus::Invalid(InvalidValidationStatus::ConsumedOnL1OrUnknown)
        }
    }

    /// Commits given transactions by removing them entirely and returning the removed
    /// transactions. Uncommitted staged transactions are rolled back to unstaged first.
    /// Performance note: This operation is linear time with both the number
    /// of known transactions and the number of committed transactions. This is assumed to be
    /// good enough while l1-handler numbers remain low, but if this changes and we need log(n)
    /// removals (amortized), replace indexmap with this (basically a BTreeIndexMap):
    /// BTreeMap<u32, TransactionEntry>, Hashmap<TransactionHash, u32> and a counter: u32, such
    /// that every new tx is inserted to the map with key counter++ and the counter is
    /// not reduced when removing entries. Once the counter reaches u32::MAX/2 we
    /// recreate the DS in Theta(n).
    pub fn commit_txs(
        &mut self,
        committed_txs: &[TransactionHash],
        rejected_txs: &[TransactionHash],
    ) {
        self.uncommitted.rollback_staging();
        self.rejected.rollback_staging();

        let mut uncommitted = IndexMap::new();
        let mut rejected = IndexMap::new();

        // Navigate transaction to rejected or uncommited.
        for (hash, entry) in self.uncommitted.txs.drain(..) {
            if rejected_txs.contains(&hash) {
                rejected.insert(hash, entry);
            } else if !committed_txs.contains(&hash) {
                uncommitted.insert(hash, entry);
            }
        }
        self.rejected.txs.extend(rejected);
        self.uncommitted.txs = uncommitted;

        // Add all committed tx hashes to the committed buffer, regardless of if they're known or
        // not, in case we haven't scraped them yet and another node did.
        self.committed.extend(committed_txs)
    }

    /// Adds a transaction to the transaction manager, return true if the transaction was
    /// successfully added. If the transaction is occupied or already committed, it will not be
    /// added, and false will be returned.
    pub fn add_tx(&mut self, tx: L1HandlerTransaction) -> bool {
        if self.committed.contains(&tx.tx_hash) || self.rejected.txs.contains_key(&tx.tx_hash) {
            return false;
        }
        self.uncommitted.insert(tx)
    }

    pub fn committed_includes(&self, tx_hashes: &[TransactionHash]) -> bool {
        tx_hashes.iter().all(|tx| self.committed.contains(tx))
    }
}
