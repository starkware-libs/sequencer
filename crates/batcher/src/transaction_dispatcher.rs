use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;
use starknet_api::executable_transaction::Transaction;
use starknet_mempool_types::communication::{MempoolClientError, SharedMempoolClient};
use thiserror::Error;

#[derive(Clone, Debug, Error)]
pub enum TransactionDispatcherError {
    #[error(transparent)]
    MempoolError(#[from] MempoolClientError),
}

#[cfg_attr(test, automock)]
#[async_trait]
pub trait TransactionDispatcher: Send + Sync {
    async fn get_txs(&self, n_txs: usize) -> Result<Vec<Transaction>, TransactionDispatcherError>;
}

pub struct ProposeTransactionDispatcher {
    pub mempool_client: SharedMempoolClient,
}

#[async_trait]
impl TransactionDispatcher for ProposeTransactionDispatcher {
    async fn get_txs(&self, n_txs: usize) -> Result<Vec<Transaction>, TransactionDispatcherError> {
        // TODO: Get also L1 transactions.
        Ok(self.mempool_client.get_txs(n_txs).await?)
    }
}
