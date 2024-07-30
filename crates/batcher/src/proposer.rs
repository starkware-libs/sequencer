use std::collections::BTreeMap;

use papyrus_config::dumping::SerializeConfig;
use serde::{Deserialize, Serialize};
use starknet_mempool_types::communication::{MempoolClientError, SharedMempoolClient};
use starknet_mempool_types::mempool_types::ThinTransaction;
use thiserror::Error;
use tracing::{info, instrument};

#[derive(Clone, Debug, PartialEq, Copy, Serialize, Deserialize)]
pub struct ProposerConfig {
    pub n_txs_to_fetch: usize,
}

impl Default for ProposerConfig {
    fn default() -> Self {
        Self { n_txs_to_fetch: 10 }
    }
}

impl SerializeConfig for ProposerConfig {
    fn dump(&self) -> BTreeMap<papyrus_config::ParamPath, papyrus_config::SerializedParam> {
        BTreeMap::from([papyrus_config::dumping::ser_param(
            "n_txs_to_fetch",
            &self.n_txs_to_fetch,
            "The maximum number of transactions to fetch from the mempool per request.",
            papyrus_config::ParamPrivacyInput::Public,
        )])
    }
}

#[derive(Clone, Debug, Error)]
pub enum ProposerError {
    #[error(transparent)]
    MempoolClientError(#[from] MempoolClientError),
}

// TODO(yair): Remove the dead_code attribute once the struct is used.
#[allow(dead_code)]
pub struct Proposer {
    pub mempool_client: SharedMempoolClient,
    pub config: ProposerConfig,
}

impl Proposer {
    #[instrument(skip(self), err)]
    pub async fn pop_more_transactions(&self) -> Result<Vec<ThinTransaction>, ProposerError> {
        let next_txs = self.mempool_client.get_txs(self.config.n_txs_to_fetch).await?;
        info!("Popped {} transactions from the mempool.", next_txs.len());
        // TODO: Pass the transactions to the builder.
        Ok(next_txs)
    }
}
