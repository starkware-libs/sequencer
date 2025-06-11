use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use apollo_mempool_types::mempool_types::{AccountState, MempoolResult};
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::rpc_transaction::InternalRpcTransaction;
use starknet_api::transaction::TransactionHash;

use crate::mempool::TransactionReference;
use crate::transaction_pool::TransactionPool;
use crate::utils::Clock;

pub type EvictionTracker = HashSet<ContractAddress>;

/// Controls access to the transaction pool, tracking and evicting lower-priority transactions when
/// the Mempool reaches capacity.
pub struct TransactionPoolController {
    tx_pool: TransactionPool,
    pub eviction_tracker: EvictionTracker,
}

impl TransactionPoolController {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        TransactionPoolController {
            tx_pool: TransactionPool::new(clock),
            eviction_tracker: EvictionTracker::new(),
        }
    }

    pub fn insert(
        &mut self,
        tx: InternalRpcTransaction,
        account_nonce: Nonce,
    ) -> MempoolResult<()> {
        let address = tx.contract_address();
        self.tx_pool.insert(tx)?;
        self.update_eviction_tracker(address, account_nonce);
        Ok(())
    }

    pub fn remove(&mut self, tx_hash: TransactionHash) -> MempoolResult<InternalRpcTransaction> {
        let tx = self.tx_pool.remove(tx_hash)?;
        let address = tx.contract_address();
        let removed_nonce = tx.nonce();

        self.update_eviction_tracker(address, removed_nonce);

        Ok(tx)
    }

    pub fn remove_up_to_account_nonce(
        &mut self,
        address: ContractAddress,
        account_nonce: Nonce,
    ) -> usize {
        let n_removed_txs = self.tx_pool.remove_up_to_nonce(address, account_nonce);
        self.update_eviction_tracker(address, account_nonce);
        n_removed_txs
    }

    pub fn get_by_tx_hash(
        &self,
        tx_hash: TransactionHash,
    ) -> MempoolResult<&InternalRpcTransaction> {
        self.tx_pool.get_by_tx_hash(tx_hash)
    }

    pub fn get_by_address_and_nonce(
        &self,
        address: ContractAddress,
        nonce: Nonce,
    ) -> Option<TransactionReference> {
        self.tx_pool.get_by_address_and_nonce(address, nonce)
    }

    pub fn contains_account(&self, address: ContractAddress) -> bool {
        self.tx_pool.contains_account(address)
    }

    pub fn get_next_eligible_tx(
        &self,
        current_account_state: AccountState,
    ) -> MempoolResult<Option<TransactionReference>> {
        self.tx_pool.get_next_eligible_tx(current_account_state)
    }

    pub fn chronological_txs_hashes(&self) -> Vec<TransactionHash> {
        self.tx_pool.chronological_txs_hashes()
    }

    pub fn size_in_bytes(&self) -> u64 {
        self.tx_pool.size_in_bytes()
    }

    pub fn len(&self) -> usize {
        self.tx_pool.len()
    }

    pub fn account_txs_sorted_by_nonce(
        &self,
        address: ContractAddress,
    ) -> impl Iterator<Item = &TransactionReference> {
        self.tx_pool.account_txs_sorted_by_nonce(address)
    }

    pub fn get_submission_time(&self, tx_hash: TransactionHash) -> MempoolResult<Instant> {
        self.tx_pool.get_submission_time(tx_hash)
    }

    pub fn remove_txs_older_than(
        &mut self,
        duration: Duration,
        exclude_txs: &HashMap<ContractAddress, Nonce>,
    ) -> Vec<TransactionReference> {
        let removed_txs = self.tx_pool.remove_txs_older_than(duration, exclude_txs);
        let mut address_to_lowest_nonce: HashMap<ContractAddress, Nonce> = HashMap::new();
        for tx in &removed_txs {
            address_to_lowest_nonce
                .entry(tx.address)
                .and_modify(|lowest_nonce| {
                    if tx.nonce < *lowest_nonce {
                        *lowest_nonce = tx.nonce;
                    }
                })
                .or_insert(tx.nonce);
        }

        for (address, lowest_removed_nonce) in address_to_lowest_nonce {
            self.update_eviction_tracker(address, lowest_removed_nonce);
        }

        removed_txs
    }

    fn has_nonce_gap(&self, address: ContractAddress, reference_nonce: Nonce) -> bool {
        let lowest_remaining_nonce = self.tx_pool.get_lowest_nonce(address);
        // If there are transactions with higher nonces than the reference nonce,
        // then there's a gap at the reference nonce.
        match lowest_remaining_nonce {
            Some(lowest_remaining_nonce) => lowest_remaining_nonce > reference_nonce,
            None => false,
        }
    }

    fn update_eviction_tracker(&mut self, address: ContractAddress, reference_nonce: Nonce) {
        if self.has_nonce_gap(address, reference_nonce) {
            self.eviction_tracker.insert(address);
        } else {
            self.eviction_tracker.remove(&address);
        }
    }

    #[cfg(test)]
    pub fn tx_pool(&self) -> HashMap<TransactionHash, InternalRpcTransaction> {
        self.tx_pool.tx_pool()
    }

    #[cfg(test)]
    pub fn with_tx_pool(tx_pool: TransactionPool) -> Self {
        Self { tx_pool, eviction_tracker: EvictionTracker::new() }
    }
}
