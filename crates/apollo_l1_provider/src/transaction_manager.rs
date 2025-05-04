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

    /// Goal:
    /// 1) Remove all new committed transaction from the uncommitted pool.
    /// 2) Move all rejected transactions from uncomitted pool to the rejected pool.
    /// 3) Add all committed transaction hashes to the committed buffer.
    ///
    /// # Performance
    /// This function has linear complexity in the number of known transactions and the
    /// number of transactions being committed. This is acceptable while the number of
    /// L1 handler transactions remains low. If higher performance becomes necessary (e.g.,
    /// requiring amortized log(n) operations), consider replacing `IndexMap` with a
    /// structure like: `BTreeMap<u32, TransactionEntry>'.
    pub fn commit_txs(
        &mut self,
        committed_txs: &[TransactionHash],
        rejected_txs: &[TransactionHash],
    ) {
        // When committing transactions, we don't need to have staged transactions.
        self.uncommitted.rollback_staging();
        self.rejected.rollback_staging();

        let mut uncommitted = IndexMap::new();
        let mut rejected = IndexMap::new();

        // Iterate over the uncommitted transactions and check if they are committed or rejected.
        for (hash, entry) in self.uncommitted.txs.drain(..) {
            // Each rejected transaction is added to the rejected pool.
            if rejected_txs.contains(&hash) {
                rejected.insert(hash, entry);
            } else if !committed_txs.contains(&hash) {
                // If a transaction is not committed or rejected, it is added to the uncommitted pool.
                uncommitted.insert(hash, entry);
            }
        }
        self.rejected.txs.extend(rejected);
        self.uncommitted.txs = uncommitted;  // Uncommitted txs was drained, so it empty now.

        // Add all committed tx hashes to the committed buffer, regardless of if they're known or
        // not, in case we haven't scraped them yet and another node did.
        self.committed.extend(committed_txs)
    }

    /// Adds a transaction to the transaction manager, return true if the transaction was added.
    /// If the transaction is occupied or already committed, it will not be added, and false will be
    /// returned.
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
