use std::sync::Arc;

use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;
use starknet_api::executable_transaction::{L1HandlerTransaction, Transaction};
use starknet_mempool_types::communication::{MempoolClientError, SharedMempoolClient};
use thiserror::Error;
use tracing::warn;

#[derive(Clone, Debug, Error)]
pub enum TransactionProviderError {
    #[error(transparent)]
    MempoolError(#[from] MempoolClientError),
}

#[derive(Debug, PartialEq)]
pub enum NextTxs {
    Txs(Vec<Transaction>),
    End,
}

#[cfg_attr(test, automock)]
#[async_trait]
pub trait TransactionProvider: Send + Sync {
    async fn get_txs(&mut self, n_txs: usize) -> Result<NextTxs, TransactionProviderError>;
}

#[cfg_attr(test, derive(Clone))]
pub struct ProposeTransactionProvider {
    pub mempool_client: SharedMempoolClient,
    // TODO: remove allow(dead_code) when L1 transactions are added.
    #[allow(dead_code)]
    pub l1_provider_client: SharedL1ProviderClient,
}

#[async_trait]
impl TransactionProvider for ProposeTransactionProvider {
    async fn get_txs(&mut self, n_txs: usize) -> Result<NextTxs, TransactionProviderError> {
        // TODO: Get also L1 transactions.
        Ok(NextTxs::Txs(
            self.mempool_client
                .get_txs(n_txs)
                .await?
                .into_iter()
                .map(Transaction::Account)
                .collect(),
        ))
    }
}

pub struct ValidateTransactionProvider {
    pub tx_receiver: tokio::sync::mpsc::Receiver<Transaction>,
}

#[async_trait]
impl TransactionProvider for ValidateTransactionProvider {
    async fn get_txs(&mut self, n_txs: usize) -> Result<NextTxs, TransactionProviderError> {
        let mut buffer = Vec::with_capacity(n_txs);
        self.tx_receiver.recv_many(&mut buffer, n_txs).await;
        // If the buffer is empty, it means that the stream was dropped, otherwise it would have
        // been waiting for transactions.
        if buffer.is_empty() {
            return Ok(NextTxs::End);
        }
        Ok(NextTxs::Txs(buffer))
    }
}

// TODO: Remove L1Provider code when the communication module of l1_provider is added.
#[cfg_attr(test, automock)]
#[async_trait]
pub trait L1ProviderClient: Send + Sync {
    #[allow(dead_code)]
    fn get_txs(&self, n_txs: usize) -> Vec<L1HandlerTransaction>;
}

pub type SharedL1ProviderClient = Arc<dyn L1ProviderClient>;

pub struct DummyL1ProviderClient;

#[async_trait]
impl L1ProviderClient for DummyL1ProviderClient {
    fn get_txs(&self, _n_txs: usize) -> Vec<L1HandlerTransaction> {
        warn!("Dummy L1 provider client is used, no L1 transactions are provided.");
        vec![]
    }
}
