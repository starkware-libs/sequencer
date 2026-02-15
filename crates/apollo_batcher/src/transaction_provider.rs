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

use crate::metrics::BATCHER_L1_PROVIDER_ERRORS;

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
    phase: TxProviderPhase,
    n_l1handler_txs_so_far: usize,
    /// Bootstrap transactions to be provided before L1/mempool transactions.
    /// These are drained as they are returned.
    bootstrap_txs: Vec<InternalConsensusTransaction>,
    /// Index of the next bootstrap transaction to return.
    bootstrap_tx_index: usize,
}

// Keeps track of which phase we're in for fetching transactions.
// The order is: Bootstrap -> L1 -> Mempool
// TODO(guyn): make the phase pub(crate) after refactoring the batcher tests.
#[derive(Clone, Debug, PartialEq)]
pub enum TxProviderPhase {
    /// Bootstrap phase: returns hardcoded bootstrap transactions.
    /// This phase only runs when the node starts with empty storage and bootstrap mode is enabled.
    Bootstrap,
    /// L1 phase: fetches L1 handler transactions.
    L1,
    /// Mempool phase: fetches transactions from the mempool.
    Mempool,
}

impl ProposeTransactionProvider {
    pub fn new(
        mempool_client: SharedMempoolClient,
        l1_provider_client: SharedL1ProviderClient,
        max_l1_handler_txs_per_block: usize,
        height: BlockNumber,
        start_phase: TxProviderPhase,
    ) -> Self {
        Self {
            mempool_client,
            l1_provider_client,
            max_l1_handler_txs_per_block,
            height,
            phase: start_phase,
            n_l1handler_txs_so_far: 0,
            bootstrap_txs: vec![],
            bootstrap_tx_index: 0,
        }
    }

    /// Create a new ProposeTransactionProvider with bootstrap transactions.
    ///
    /// The provider will first return bootstrap transactions, then transition to
    /// L1 and mempool phases.
    pub fn new_with_bootstrap(
        mempool_client: SharedMempoolClient,
        l1_provider_client: SharedL1ProviderClient,
        max_l1_handler_txs_per_block: usize,
        height: BlockNumber,
        bootstrap_txs: Vec<InternalConsensusTransaction>,
    ) -> Self {
        let start_phase =
            if bootstrap_txs.is_empty() { TxProviderPhase::L1 } else { TxProviderPhase::Bootstrap };
        Self {
            mempool_client,
            l1_provider_client,
            max_l1_handler_txs_per_block,
            height,
            phase: start_phase,
            n_l1handler_txs_so_far: 0,
            bootstrap_txs,
            bootstrap_tx_index: 0,
        }
    }

    /// Get bootstrap transactions for this block.
    ///
    /// Returns up to n_txs bootstrap transactions, advancing the internal index.
    fn get_bootstrap_txs(&mut self, n_txs: usize) -> Vec<InternalConsensusTransaction> {
        let remaining = self.bootstrap_txs.len() - self.bootstrap_tx_index;
        let to_take = std::cmp::min(n_txs, remaining);
        let result: Vec<_> =
            self.bootstrap_txs[self.bootstrap_tx_index..self.bootstrap_tx_index + to_take].to_vec();
        self.bootstrap_tx_index += to_take;
        result
    }

    /// Check if all bootstrap transactions have been provided.
    fn bootstrap_exhausted(&self) -> bool {
        self.bootstrap_tx_index >= self.bootstrap_txs.len()
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

        // Phase 1: Bootstrap transactions (if any)
        if self.phase == TxProviderPhase::Bootstrap {
            let mut bootstrap_txs = self.get_bootstrap_txs(n_txs);
            txs.append(&mut bootstrap_txs);

            // Transition to L1 phase when bootstrap is exhausted
            if self.bootstrap_exhausted() {
                self.phase = TxProviderPhase::L1;
            }

            if txs.len() == n_txs {
                return Ok(txs);
            }
        }

        // Phase 2: L1 handler transactions
        if self.phase == TxProviderPhase::L1 {
            let n_l1handler_txs_to_get = min(
                self.max_l1_handler_txs_per_block - self.n_l1handler_txs_so_far,
                n_txs - txs.len(),
            );
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

        // Phase 3: Mempool transactions
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

/// A simple transaction provider that only returns bootstrap transactions.
/// Used during the bootstrap phase before consensus starts.
pub struct BootstrapOnlyTransactionProvider {
    txs: Vec<InternalConsensusTransaction>,
    index: usize,
}

impl BootstrapOnlyTransactionProvider {
    pub fn new(txs: Vec<InternalConsensusTransaction>) -> Self {
        Self { txs, index: 0 }
    }
}

#[async_trait]
impl TransactionProvider for BootstrapOnlyTransactionProvider {
    async fn get_txs(&mut self, n_txs: usize) -> TransactionProviderResult<NextTxs> {
        let remaining = self.txs.len() - self.index;
        let to_take = min(n_txs, remaining);
        let result: Vec<_> = self.txs[self.index..self.index + to_take].to_vec();
        self.index += to_take;
        Ok(result)
    }

    async fn get_final_n_executed_txs(&mut self) -> Option<usize> {
        None
    }

    #[cfg(test)]
    fn phase(&self) -> TxProviderPhase {
        TxProviderPhase::Bootstrap
    }
}
