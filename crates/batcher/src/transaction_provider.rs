use std::cmp::min;
use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::executable_transaction::{L1HandlerTransaction, Transaction};
use starknet_mempool_types::communication::{MempoolClientError, SharedMempoolClient};
use thiserror::Error;
use tracing::warn;
use validator::Validate;

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

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct TransactionProviderConfig {
    pub max_l1_handler_txs_per_block: usize,
}

impl Default for TransactionProviderConfig {
    fn default() -> Self {
        Self { max_l1_handler_txs_per_block: 100 }
    }
}

impl SerializeConfig for TransactionProviderConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([ser_param(
            "max_l1_handler_txs_per_block",
            &self.max_l1_handler_txs_per_block,
            "The maximum number of L1 handler transactions to include in a block.",
            ParamPrivacyInput::Public,
        )])
    }
}

pub struct ProposeTransactionProvider {
    pub config: TransactionProviderConfig,
    pub mempool_client: SharedMempoolClient,
    pub l1_provider_client: SharedL1ProviderClient,
    phase: TxProviderPhase,
}

// Keeps track of whether we need to fetch L1 handler transactions or mempool transactions.
enum TxProviderPhase {
    L1 { n_txs_so_far: usize },
    Mempool,
}

impl ProposeTransactionProvider {
    pub fn new(
        config: TransactionProviderConfig,
        mempool_client: SharedMempoolClient,
        l1_provider_client: SharedL1ProviderClient,
    ) -> Self {
        Self {
            config,
            mempool_client,
            l1_provider_client,
            phase: TxProviderPhase::L1 { n_txs_so_far: 0 },
        }
    }

    fn get_l1_handler_txs(&mut self, n_txs: usize) -> Vec<Transaction> {
        let TxProviderPhase::L1 { mut n_txs_so_far } = self.phase else {
            return vec![];
        };
        let n_l1_txs_to_get = min(self.config.max_l1_handler_txs_per_block - n_txs_so_far, n_txs);
        if n_l1_txs_to_get == 0 {
            self.phase = TxProviderPhase::Mempool;
            return vec![];
        }
        let txs: Vec<_> = self
            .l1_provider_client
            .get_txs(n_l1_txs_to_get)
            .into_iter()
            .map(Transaction::L1Handler)
            .collect();
        n_txs_so_far += txs.len();
        if txs.len() < n_l1_txs_to_get || n_txs_so_far == self.config.max_l1_handler_txs_per_block {
            self.phase = TxProviderPhase::Mempool;
        } else {
            self.phase = TxProviderPhase::L1 { n_txs_so_far };
        }
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
        let mut next_txs = self.get_l1_handler_txs(n_txs);
        if next_txs.len() == n_txs {
            return Ok(NextTxs::Txs(next_txs));
        }
        let mempool_txs = self.get_mempool_txs(n_txs - next_txs.len()).await?;
        next_txs.extend(mempool_txs);
        Ok(NextTxs::Txs(next_txs))
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

// TODO: Remove L1Provider code when the communication module of l1-provider is added.
#[cfg_attr(test, automock)]
#[async_trait]
pub trait L1ProviderClient: Send + Sync {
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
