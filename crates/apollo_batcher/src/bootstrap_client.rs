use apollo_batcher_types::bootstrap_types::{BootstrapRequest, BootstrapResponse, BootstrapState};
use starknet_api::rpc_transaction::{InternalRpcTransaction, RpcTransaction};
use tracing::warn;

use crate::bootstrap_server::{BatcherStorageReaderRequest, BatcherStorageReaderResponse};

/// Result of bootstrap validation.
pub enum BootstrapValidation {
    /// Not in bootstrap mode; proceed with normal validation.
    NotBootstrapping,
    /// The transaction is a valid bootstrap transaction; skip normal validation.
    ValidBootstrapTx,
}

/// HTTP client for querying the batcher storage reader's bootstrap endpoint.
#[derive(Clone)]
pub struct BootstrapClient {
    client: reqwest::Client,
    url: String,
}

impl BootstrapClient {
    /// Creates a new bootstrap client if the URL is non-empty.
    pub fn new(batcher_storage_reader_url: &str) -> Option<Self> {
        if batcher_storage_reader_url.is_empty() {
            return None;
        }
        Some(Self {
            client: reqwest::Client::new(),
            url: format!("{}/storage/query", batcher_storage_reader_url),
        })
    }

    pub async fn get_bootstrap_state(&self) -> Result<BootstrapState, String> {
        let request = BatcherStorageReaderRequest::Bootstrap(BootstrapRequest::GetBootstrapState);
        let body =
            serde_json::to_string(&request).map_err(|e| format!("Serialization error: {e}"))?;

        let resp = self
            .client
            .post(&self.url)
            .header("content-type", "application/json")
            .body(body)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("HTTP request returned status {}", resp.status()));
        }

        let text = resp.text().await.map_err(|e| format!("Failed to read response body: {e}"))?;
        let response: BatcherStorageReaderResponse =
            serde_json::from_str(&text).map_err(|e| format!("Failed to parse response: {e}"))?;

        match response {
            BatcherStorageReaderResponse::Bootstrap(BootstrapResponse::BootstrapState(state)) => {
                Ok(state)
            }
            _ => Err("Unexpected response type for GetBootstrapState".to_string()),
        }
    }

    pub async fn get_bootstrap_transactions(&self) -> Result<Vec<RpcTransaction>, String> {
        let request =
            BatcherStorageReaderRequest::Bootstrap(BootstrapRequest::GetBootstrapTransactions);
        let body =
            serde_json::to_string(&request).map_err(|e| format!("Serialization error: {e}"))?;

        let resp = self
            .client
            .post(&self.url)
            .header("content-type", "application/json")
            .body(body)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("HTTP request returned status {}", resp.status()));
        }

        let text = resp.text().await.map_err(|e| format!("Failed to read response body: {e}"))?;
        let response: BatcherStorageReaderResponse =
            serde_json::from_str(&text).map_err(|e| format!("Failed to parse response: {e}"))?;

        match response {
            BatcherStorageReaderResponse::Bootstrap(BootstrapResponse::BootstrapTransactions(
                txs,
            )) => Ok(txs),
            _ => Err("Unexpected response type for GetBootstrapTransactions".to_string()),
        }
    }

    /// Validates an `RpcTransaction` against the expected bootstrap transactions.
    ///
    /// Returns `ValidBootstrapTx` if bootstrapping is active and the tx matches,
    /// `NotBootstrapping` if not in bootstrap, or an error if the tx doesn't match.
    pub async fn validate_bootstrap_tx(
        &self,
        tx: &RpcTransaction,
    ) -> Result<BootstrapValidation, String> {
        let state = self.get_bootstrap_state().await?;
        match state {
            BootstrapState::NotInBootstrap => Ok(BootstrapValidation::NotBootstrapping),
            _ => {
                let expected_txs = self.get_bootstrap_transactions().await?;
                if expected_txs.contains(tx) {
                    Ok(BootstrapValidation::ValidBootstrapTx)
                } else {
                    warn!(
                        "Received transaction during bootstrap that doesn't match expected set \
                         (state={:?}, expected_count={})",
                        state,
                        expected_txs.len()
                    );
                    Err(format!(
                        "Transaction does not match expected bootstrap transactions for state {:?}",
                        state
                    ))
                }
            }
        }
    }

    /// Validates an `InternalRpcTransaction` against the expected bootstrap transactions.
    ///
    /// Comparison is done by (contract_address, nonce) since `InternalRpcTransaction` and
    /// `RpcTransaction` are different types.
    pub async fn validate_bootstrap_internal_tx(
        &self,
        tx: &InternalRpcTransaction,
    ) -> Result<BootstrapValidation, String> {
        let state = self.get_bootstrap_state().await?;
        match state {
            BootstrapState::NotInBootstrap => Ok(BootstrapValidation::NotBootstrapping),
            _ => {
                let expected_txs = self.get_bootstrap_transactions().await?;
                let tx_address = tx.contract_address();
                let tx_nonce = tx.nonce();

                let matches = expected_txs.iter().any(|expected| {
                    let expected_address = expected.calculate_sender_address().ok();
                    let expected_nonce = expected.nonce();
                    expected_address.as_ref() == Some(&tx_address) && *expected_nonce == tx_nonce
                });

                if matches {
                    Ok(BootstrapValidation::ValidBootstrapTx)
                } else {
                    warn!(
                        "Received transaction in mempool during bootstrap that doesn't match \
                         expected set (state={:?}, tx_address={:?}, tx_nonce={:?}, \
                         expected_count={})",
                        state,
                        tx_address,
                        tx_nonce,
                        expected_txs.len()
                    );
                    Err(format!(
                        "Transaction does not match expected bootstrap transactions for state {:?}",
                        state
                    ))
                }
            }
        }
    }
}
