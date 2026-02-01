use std::cmp::min;
use std::vec;

use apollo_l1_provider_types::errors::L1ProviderClientError;
use apollo_l1_provider_types::{
    InvalidValidationStatus as L1InvalidValidationStatus,
    SharedL1ProviderClient,
    ValidationStatus as L1ValidationStatus,
};
use apollo_mempool_types::communication::{MempoolClientError, SharedMempoolClient};
use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;
use starknet_api::block::BlockNumber;
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::transaction::TransactionHash;
use thiserror::Error;
use tokio::time::Duration;

use crate::metrics::BATCHER_L1_PROVIDER_ERRORS;
use crate::echonet_tx_filter_client::{EchonetTxFilterClient, EchonetTxFilterClientTrait};

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

pub type NextTxs = Vec<InternalConsensusTransaction>;

#[cfg_attr(test, automock)]
#[async_trait]
pub trait TransactionProvider: Send {
    async fn get_txs(&mut self, n_txs: usize) -> TransactionProviderResult<NextTxs>;
    /// In validate mode ([ValidateTransactionProvider]) returns the final number of transactions
    /// in the block once it is known, or `None` if it is not known yet.
    /// Once `Some()` is returned for the first time, future calls to this method may return `None`.
    /// Returns `None` in propose mode ([ProposeTransactionProvider]).
    async fn get_final_n_executed_txs(&mut self) -> Option<usize>;

    // TODO(guyn): remove this after refactoring the batcher tests.
    #[cfg(test)]
    fn phase(&self) -> TxProviderPhase;
}

#[derive(Clone)]
pub struct ProposeTransactionProvider {
    pub mempool_client: SharedMempoolClient,
    pub l1_provider_client: SharedL1ProviderClient,
    pub max_l1_handler_txs_per_block: usize,
    pub height: BlockNumber,
    pub block_timestamp_seconds: u64,
    pub echonet_tx_timestamp_filter_enabled: bool,
    echonet_tx_filter_client: Option<EchonetTxFilterClient>,
    phase: TxProviderPhase,
    n_l1handler_txs_so_far: usize,
}

// Keeps track of whether we need to fetch L1 handler transactions or mempool transactions.
// TODO(guyn): make the phase pub(crate) after refactoring the batcher tests.
#[derive(Clone, Debug, PartialEq)]
pub enum TxProviderPhase {
    L1,
    Mempool,
}

impl ProposeTransactionProvider {
    pub fn new(
        mempool_client: SharedMempoolClient,
        l1_provider_client: SharedL1ProviderClient,
        max_l1_handler_txs_per_block: usize,
        height: BlockNumber,
        block_timestamp_seconds: u64,
        start_phase: TxProviderPhase,
        echonet_tx_timestamp_filter_enabled: bool,
        echonet_tx_timestamp_filter_timeout: Duration,
        recorder_url: apollo_config::secrets::Sensitive<url::Url>,
    ) -> Self {
        let echonet_tx_filter_client = if echonet_tx_timestamp_filter_enabled {
            Some(EchonetTxFilterClient::new(recorder_url, echonet_tx_timestamp_filter_timeout))
        } else {
            None
        };
        Self {
            mempool_client,
            l1_provider_client,
            max_l1_handler_txs_per_block,
            height,
            block_timestamp_seconds,
            echonet_tx_timestamp_filter_enabled,
            echonet_tx_filter_client,
            phase: start_phase,
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
            .await
            .inspect_err(|_err| {
                BATCHER_L1_PROVIDER_ERRORS.increment(1);
            })
            .unwrap_or_default()
            .into_iter()
            .map(InternalConsensusTransaction::L1Handler)
            .collect())
    }

    async fn get_mempool_txs(
        &mut self,
        n_txs: usize,
    ) -> TransactionProviderResult<Vec<InternalConsensusTransaction>> {
        let mempool_txs: Vec<InternalConsensusTransaction> = self
            .mempool_client
            .get_txs(n_txs)
            .await?
            .into_iter()
            .map(InternalConsensusTransaction::RpcTransaction)
            .collect();

        if !self.echonet_tx_timestamp_filter_enabled {
            return Ok(mempool_txs);
        }
        let Some(client) = &self.echonet_tx_filter_client else {
            return Ok(mempool_txs);
        };
        if mempool_txs.is_empty() {
            return Ok(mempool_txs);
        }

        let hashes: Vec<TransactionHash> = mempool_txs.iter().map(|t| t.tx_hash()).collect();
        match client.allowed_txs_for_timestamp(self.block_timestamp_seconds, &hashes).await {
            Ok(allowed_hex) => {
                let filtered: Vec<InternalConsensusTransaction> = mempool_txs
                    .into_iter()
                    .filter(|t| allowed_hex.contains(&t.tx_hash().0.to_hex_string()))
                    .collect();
                Ok(filtered)
            }
            Err(err) => {
                // Fail-open: if echonet filter is unavailable, do not block block production.
                tracing::warn!("Echonet tx timestamp filter failed; allowing all txs: {err}");
                Ok(mempool_txs)
            }
        }
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
                return Ok(txs);
            }
        }

        let mut mempool_txs = self.get_mempool_txs(n_txs - txs.len()).await?;
        txs.append(&mut mempool_txs);
        Ok(txs)
    }

    async fn get_final_n_executed_txs(&mut self) -> Option<usize> {
        None
    }

    // TODO(guyn): remove this after refactoring the batcher tests.
    #[cfg(test)]
    fn phase(&self) -> TxProviderPhase {
        self.phase.clone()
    }
}

pub struct ValidateTransactionProvider {
    tx_receiver: tokio::sync::mpsc::Receiver<InternalConsensusTransaction>,
    final_n_executed_txs_receiver: tokio::sync::oneshot::Receiver<usize>,
    l1_provider_client: SharedL1ProviderClient,
    height: BlockNumber,
}

impl ValidateTransactionProvider {
    pub fn new(
        tx_receiver: tokio::sync::mpsc::Receiver<InternalConsensusTransaction>,
        final_n_executed_txs_receiver: tokio::sync::oneshot::Receiver<usize>,
        l1_provider_client: SharedL1ProviderClient,
        height: BlockNumber,
    ) -> Self {
        Self { tx_receiver, final_n_executed_txs_receiver, l1_provider_client, height }
    }
}

#[async_trait]
impl TransactionProvider for ValidateTransactionProvider {
    async fn get_txs(&mut self, n_txs: usize) -> TransactionProviderResult<NextTxs> {
        assert!(n_txs > 0, "The number of transactions requested must be greater than zero.");

        if self.tx_receiver.is_empty() {
            // Return immediately to avoid blocking the caller.
            return Ok(vec![]);
        }

        let mut buffer = Vec::with_capacity(n_txs);
        self.tx_receiver.recv_many(&mut buffer, n_txs).await;

        for tx in &buffer {
            if let InternalConsensusTransaction::L1Handler(tx) = tx {
                let l1_validation_status = self
                    .l1_provider_client
                    .validate(tx.tx_hash, self.height)
                    .await
                    .inspect_err(|_err| {
                        BATCHER_L1_PROVIDER_ERRORS.increment(1);
                    })
                    .unwrap_or(L1ValidationStatus::Invalid(
                        L1InvalidValidationStatus::L1ProviderError,
                    ));
                if let L1ValidationStatus::Invalid(validation_status) = l1_validation_status {
                    return Err(TransactionProviderError::L1HandlerTransactionValidationFailed {
                        tx_hash: tx.tx_hash,
                        validation_status,
                    });
                }
            }
        }
        Ok(buffer)
    }

    async fn get_final_n_executed_txs(&mut self) -> Option<usize> {
        // Return None if the receiver is empty or closed unexpectedly.
        self.final_n_executed_txs_receiver.try_recv().ok()
    }

    // TODO(guyn): remove this after refactoring the batcher tests.
    #[cfg(test)]
    fn phase(&self) -> TxProviderPhase {
        panic!("Phase is only relevant to proposing transactions.")
    }
}
