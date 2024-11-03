use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;
use starknet_api::executable_transaction::Transaction;
use starknet_mempool_types::communication::{MempoolClientError, SharedMempoolClient};
use thiserror::Error;

#[derive(Clone, Debug, Error)]
pub enum TransactionProviderError {
    #[error(transparent)]
    MempoolError(#[from] MempoolClientError),
    #[error(transparent)]
    ChannelSendError(#[from] tokio::sync::mpsc::error::SendError<Transaction>),
}

#[cfg_attr(test, automock)]
#[async_trait]
pub trait TransactionProvider: Send + Sync {
    async fn get_txs(&mut self, n_txs: usize) -> Result<TxStream, TransactionProviderError>;
}

pub struct ProposeTransactionProvider {
    pub mempool_client: SharedMempoolClient,
}

#[async_trait]
impl TransactionProvider for ProposeTransactionProvider {
    async fn get_txs(&mut self, n_txs: usize) -> Result<TxStream, TransactionProviderError> {
        // TODO: Get also L1 transactions.
        Ok(TxStream::Txs(self.mempool_client.get_txs(n_txs).await?))
    }
}

#[derive(Debug, PartialEq)]
pub enum TxStream {
    Txs(Vec<Transaction>),
    TxStreamDone,
}

pub struct ValidateTransactionProvider {
    pub tx_receiver: tokio::sync::mpsc::Receiver<Transaction>,
}

#[async_trait]
impl TransactionProvider for ValidateTransactionProvider {
    async fn get_txs(&mut self, n_txs: usize) -> Result<TxStream, TransactionProviderError> {
        let mut buffer = Vec::with_capacity(n_txs);
        self.tx_receiver.recv_many(&mut buffer, n_txs).await;
        // If the buffer is empty, it means that the stream was dropped, otherwise it would have
        // been waiting for transactions.
        if buffer.is_empty() {
            return Ok(TxStream::TxStreamDone);
        }
        Ok(TxStream::Txs(buffer))
    }
}
