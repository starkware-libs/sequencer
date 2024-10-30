use async_trait::async_trait;
use starknet_api::executable_transaction::Transaction;
use starknet_mempool_types::communication::{MempoolClientError, SharedMempoolClient};
use thiserror::Error;

#[derive(Clone, Debug, Error)]
pub enum TransactionProviderError {
    #[error(transparent)]
    MempoolError(#[from] MempoolClientError),
}

#[async_trait]
pub trait TransactionProvider: Send + Sync {
    async fn get_txs(&self, n_txs: usize) -> Result<Vec<Transaction>, TransactionProviderError>;
}

pub struct ProposeTransactionProvider {
    pub mempool_client: SharedMempoolClient,
}

#[async_trait]
impl TransactionProvider for ProposeTransactionProvider {
    async fn get_txs(&self, n_txs: usize) -> Result<Vec<Transaction>, TransactionProviderError> {
        // TODO: Get also L1 transactions.
        Ok(self.mempool_client.get_txs(n_txs).await?)
    }
}
