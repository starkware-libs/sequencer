use std::sync::Arc;

use apollo_mempool_types::mempool_types::MempoolResult;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::rpc_transaction::InternalRpcTransaction;
use starknet_api::transaction::TransactionHash;

use crate::eviction_tracker::EvictionTracker;
use crate::transaction_pool::TransactionPool;
use crate::utils::Clock;
pub struct TransactionController {
    pub tx_pool: TransactionPool,
    pub eviction_tracker: EvictionTracker,
}

impl TransactionController {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        TransactionController {
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
        self.eviction_tracker.update(address, self.has_nonce_gap(address, account_nonce));
        Ok(())
    }

    pub fn remove(&mut self, tx_hash: TransactionHash) -> MempoolResult<InternalRpcTransaction> {
        let tx_result = self.tx_pool.remove(tx_hash);
        if let Ok(tx) = &tx_result {
            let address = tx.contract_address();
            self.eviction_tracker.update(address, self.has_nonce_gap(address, tx.nonce()));
        }
        tx_result
    }

    pub fn remove_up_to_nonce(&mut self, address: ContractAddress, nonce: Nonce) -> usize {
        let n_removed_txs = self.tx_pool.remove_up_to_nonce(address, nonce);
        self.eviction_tracker.update(address, self.has_nonce_gap(address, nonce));
        n_removed_txs
    }

    fn has_nonce_gap(&self, address: ContractAddress, account_nonce: Nonce) -> bool {
        self.tx_pool.get_by_address_and_nonce(address, account_nonce).is_none()
            && self.tx_pool.contains_account(address)
    }
}
