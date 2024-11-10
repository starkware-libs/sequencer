use std::cmp::min;
use std::sync::Arc;
use std::vec;

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

pub struct ProposeTransactionProvider {
    pub mempool_client: SharedMempoolClient,
    pub l1_provider_client: SharedL1ProviderClient,
    pub max_l1_handler_txs_per_block: usize,
    phase: TxProviderPhase,
    n_l1handler_txs_so_far: usize,
}

// Keeps track of whether we need to fetch L1 handler transactions or mempool transactions.
#[derive(Debug, PartialEq)]
enum TxProviderPhase {
    L1,
    Mempool,
}

impl ProposeTransactionProvider {
    pub fn new(
        mempool_client: SharedMempoolClient,
        l1_provider_client: SharedL1ProviderClient,
        max_l1_handler_txs_per_block: usize,
    ) -> Self {
        Self {
            mempool_client,
            l1_provider_client,
            max_l1_handler_txs_per_block,
            phase: TxProviderPhase::L1,
            n_l1handler_txs_so_far: 0,
        }
    }

    fn get_l1_handler_txs(&mut self, n_txs: usize) -> Vec<Transaction> {
        let txs: Vec<_> = self
            .l1_provider_client
            .get_txs(n_txs)
            .into_iter()
            .map(Transaction::L1Handler)
            .collect();
        txs
    }

    async fn get_mempool_txs(
        &mut self,
        n_txs: usize,
    ) -> Result<Vec<Transaction>, TransactionProviderError> {
        Ok(self
            .mempool_client
            .get_txs(n_txs)
            .await?
            .into_iter()
            .map(Transaction::Account)
            .collect())
    }
}

#[async_trait]
impl TransactionProvider for ProposeTransactionProvider {
    async fn get_txs(&mut self, n_txs: usize) -> Result<NextTxs, TransactionProviderError> {
        let mut txs = vec![];
        if self.phase == TxProviderPhase::L1 {
            let n_l1handler_txs_to_get =
                min(self.max_l1_handler_txs_per_block - self.n_l1handler_txs_so_far, n_txs);
            let mut l1handler_txs = self.get_l1_handler_txs(n_l1handler_txs_to_get);
            self.n_l1handler_txs_so_far += l1handler_txs.len();
            let no_more_l1handler_in_provider = l1handler_txs.len() < n_l1handler_txs_to_get;
            let reached_max_l1handler_txs_in_block =
                self.n_l1handler_txs_so_far == self.max_l1_handler_txs_per_block;
            if no_more_l1handler_in_provider || reached_max_l1handler_txs_in_block {
                self.phase = TxProviderPhase::Mempool;
            }
            txs.append(&mut l1handler_txs);
            if txs.len() == n_txs {
                return Ok(NextTxs::Txs(txs));
            }
        }

        let mut mempool_txs = self.get_mempool_txs(n_txs - txs.len()).await?;
        txs.append(&mut mempool_txs);
        Ok(NextTxs::Txs(txs))
    }
}

pub struct ValidateTransactionProvider {
    pub tx_receiver: tokio::sync::mpsc::Receiver<Transaction>,
    pub l1_provider_client: SharedL1ProviderClient,
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
        buffer.iter().all(|tx| match tx {
            Transaction::L1Handler(tx) => self.l1_provider_client.validate(tx),
            Transaction::Account(_) => true,
        });
        Ok(NextTxs::Txs(buffer))
    }
}

// TODO: Remove L1Provider code when the communication module of l1-provider is added.
#[cfg_attr(test, automock)]
#[async_trait]
pub trait L1ProviderClient: Send + Sync {
    fn get_txs(&self, n_txs: usize) -> Vec<L1HandlerTransaction>;
    fn validate(&self, tx: &L1HandlerTransaction) -> bool;
}

pub type SharedL1ProviderClient = Arc<dyn L1ProviderClient>;

pub struct DummyL1ProviderClient;

#[async_trait]
impl L1ProviderClient for DummyL1ProviderClient {
    fn get_txs(&self, _n_txs: usize) -> Vec<L1HandlerTransaction> {
        warn!("Dummy L1 provider client is used, no L1 transactions are provided.");
        vec![]
    }

    fn validate(&self, _tx: &L1HandlerTransaction) -> bool {
        warn!("Dummy L1 provider client is used, tx is not really validated.");
        true
    }
}
