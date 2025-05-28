use std::sync::Arc;

use apollo_mempool_types::mempool_types::MempoolResult;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::rpc_transaction::InternalRpcTransaction;
use starknet_api::transaction::TransactionHash;

use crate::transaction_pool::TransactionPool;
use crate::utils::Clock;
pub struct TransactionController {
    pub tx_pool: TransactionPool,
}

impl TransactionController {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        TransactionController { tx_pool: TransactionPool::new(clock) }
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
}
