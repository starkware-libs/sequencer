use std::cmp::min;
use std::vec;

use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;
use starknet_api::block::BlockNumber;
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_l1_provider_types::errors::L1ProviderClientError;
use starknet_l1_provider_types::{
    InvalidValidationStatus as L1InvalidValidationStatus,
    SharedL1ProviderClient,
    ValidationStatus as L1ValidationStatus,
};
use starknet_mempool_types::communication::{MempoolClientError, SharedMempoolClient};
use thiserror::Error;

type TransactionProviderResult<T> = Result<T, TransactionProviderError>;

#[derive(Clone, Debug, Error)]
pub enum TransactionProviderError {
    #[error(transparent)]
    MempoolError(#[from] MempoolClientError),
    #[error(
        "L1Handler transaction validation failed for tx with hash {} status {:?}.",
        tx_hash.0.to_hex_string(),
        validation_status
    )]
    L1HandlerTransactionValidationFailed {
        tx_hash: TransactionHash,
        validation_status: L1InvalidValidationStatus,
    },
    #[error(transparent)]
    L1ProviderError(#[from] L1ProviderClientError),
}

#[derive(Debug, PartialEq)]
pub enum NextTxs {
    Txs(Vec<InternalConsensusTransaction>),
    End,
}

#[cfg_attr(test, automock)]
#[async_trait]
pub trait TransactionProvider: Send {
    async fn get_txs(&mut self, n_txs: usize) -> TransactionProviderResult<NextTxs>;
}

#[derive(Clone)]
pub struct ProposeTransactionProvider {
    pub mempool_client: SharedMempoolClient,
    pub l1_provider_client: SharedL1ProviderClient,
    pub max_l1_handler_txs_per_block: usize,
    pub height: BlockNumber,
    phase: TxProviderPhase,
    n_l1handler_txs_so_far: usize,
}

// Keeps track of whether we need to fetch L1 handler transactions or mempool transactions.
#[derive(Clone, Debug, PartialEq)]
enum TxProviderPhase {
    L1,
    Mempool,
}

impl ProposeTransactionProvider {
    pub fn new(
        mempool_client: SharedMempoolClient,
        l1_provider_client: SharedL1ProviderClient,
        max_l1_handler_txs_per_block: usize,
        height: BlockNumber,
    ) -> Self {
        Self {
            mempool_client,
            l1_provider_client,
            max_l1_handler_txs_per_block,
            height,
            phase: TxProviderPhase::L1,
            n_l1handler_txs_so_far: 0,
        }
    }

    async fn get_l1_handler_txs(
        &mut self,
        n_txs: usize,
    ) -> TransactionProviderResult<Vec<InternalConsensusTransaction>> {
        Ok(self
            .l1_provider_client
            .get_txs(n_txs, self.height)
            .await?
            .into_iter()
            .map(InternalConsensusTransaction::L1Handler)
            .collect())
    }

    async fn get_mempool_txs(
        &mut self,
        n_txs: usize,
    ) -> TransactionProviderResult<Vec<InternalConsensusTransaction>> {
        Ok(self
            .mempool_client
            .get_txs(n_txs)
            .await?
            .into_iter()
            .map(InternalConsensusTransaction::RpcTransaction)
            .collect())
    }
}

#[async_trait]
impl TransactionProvider for ProposeTransactionProvider {
    async fn get_txs(&mut self, n_txs: usize) -> TransactionProviderResult<NextTxs> {
        assert!(n_txs > 0, "The number of transactions requested must be greater than zero.");
        let mut txs = vec![];
        if self.phase == TxProviderPhase::L1 {
            let n_l1handler_txs_to_get =
                min(self.max_l1_handler_txs_per_block - self.n_l1handler_txs_so_far, n_txs);
            let mut l1handler_txs = self.get_l1_handler_txs(n_l1handler_txs_to_get).await?;
            self.n_l1handler_txs_so_far += l1handler_txs.len();

            // Determine whether we need to switch to mempool phase.
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
    pub tx_receiver: tokio::sync::mpsc::Receiver<InternalConsensusTransaction>,
    pub l1_provider_client: SharedL1ProviderClient,
    pub height: BlockNumber,
}

#[async_trait]
impl TransactionProvider for ValidateTransactionProvider {
    async fn get_txs(&mut self, n_txs: usize) -> TransactionProviderResult<NextTxs> {
        assert!(n_txs > 0, "The number of transactions requested must be greater than zero.");
        let mut buffer = Vec::with_capacity(n_txs);
        self.tx_receiver.recv_many(&mut buffer, n_txs).await;
        // If the buffer is empty, it means that the stream was dropped, otherwise it would have
        // been waiting for transactions.
        if buffer.is_empty() {
            return Ok(NextTxs::End);
        }
        for tx in &buffer {
            if let InternalConsensusTransaction::L1Handler(tx) = tx {
                let l1_validation_status =
                    self.l1_provider_client.validate(tx.tx_hash, self.height).await?;
                if let L1ValidationStatus::Invalid(validation_status) = l1_validation_status {
                    return Err(TransactionProviderError::L1HandlerTransactionValidationFailed {
                        tx_hash: tx.tx_hash,
                        validation_status,
                    });
                }
            }
        }
        Ok(NextTxs::Txs(buffer))
    }
}
