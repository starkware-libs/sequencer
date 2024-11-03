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
    #[error(transparent)]
    ChannelSendError(#[from] tokio::sync::mpsc::error::SendError<Transaction>),
}

#[derive(Debug, PartialEq)]
pub enum TransactionEvent {
    Transaction(Transaction),
    Finish,
}

#[cfg_attr(test, automock)]
#[async_trait]
pub trait TransactionDispatcher: Send + Sync {
    async fn get_txs(
        &mut self,
        n_txs: usize,
    ) -> Result<Vec<TransactionEvent>, TransactionDispatcherError>;
}

pub struct ProposeTransactionDispatcher {
    pub mempool_client: SharedMempoolClient,
}

#[async_trait]
impl TransactionDispatcher for ProposeTransactionDispatcher {
    async fn get_txs(
        &mut self,
        n_txs: usize,
    ) -> Result<Vec<TransactionEvent>, TransactionDispatcherError> {
        // TODO: Get also L1 transactions.
        Ok(self
            .mempool_client
            .get_txs(n_txs)
            .await?
            .into_iter()
            .map(TransactionEvent::Transaction)
            .collect())
    }
}

pub struct ValidateTransactionDispatcher {
    pub tx_receiver: tokio::sync::mpsc::Receiver<TransactionEvent>,
}

#[async_trait]
impl TransactionDispatcher for ValidateTransactionDispatcher {
    async fn get_txs(
        &mut self,
        n_txs: usize,
    ) -> Result<Vec<TransactionEvent>, TransactionDispatcherError> {
        let mut buffer = Vec::with_capacity(n_txs);
        self.tx_receiver.recv_many(&mut buffer, n_txs).await;
        Ok(buffer)
    }
}
