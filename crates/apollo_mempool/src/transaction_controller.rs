use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use apollo_mempool_types::mempool_types::{AccountState, MempoolResult};
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::rpc_transaction::InternalRpcTransaction;
use starknet_api::transaction::TransactionHash;

use crate::mempool::TransactionReference;
use crate::transaction_pool::TransactionPool;
use crate::utils::Clock;

/// Controls access to the transaction pool, tracking and evicting lower-priority transactions when
/// the Mempool reaches capacity.
pub struct TransactionPoolController {
    tx_pool: TransactionPool,
}

impl TransactionPoolController {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        TransactionPoolController { tx_pool: TransactionPool::new(clock) }
    }

    pub fn insert(&mut self, tx: InternalRpcTransaction) -> MempoolResult<()> {
        self.tx_pool.insert(tx)
    }

    pub fn remove(&mut self, tx_hash: TransactionHash) -> MempoolResult<InternalRpcTransaction> {
        self.tx_pool.remove(tx_hash)
    }

    pub fn remove_up_to_nonce(&mut self, address: ContractAddress, nonce: Nonce) -> usize {
        self.tx_pool.remove_up_to_nonce(address, nonce)
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
        self.tx_pool.remove_txs_older_than(duration, exclude_txs)
    }

    #[cfg(test)]
    pub fn tx_pool(&self) -> HashMap<TransactionHash, InternalRpcTransaction> {
        self.tx_pool.tx_pool()
    }

    #[cfg(test)]
    pub fn with_tx_pool(tx_pool: TransactionPool) -> Self {
        Self { tx_pool }
    }
}
