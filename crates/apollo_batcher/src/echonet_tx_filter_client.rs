use std::collections::HashSet;
use std::time::Duration;

use apollo_config::secrets::Sensitive;
use async_trait::async_trait;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use starknet_api::transaction::TransactionHash;
use thiserror::Error;
use tracing::{debug, warn};
use url::Url;

/// Endpoint on echonet/echo_center. (Not implemented yet on the python side.)
pub const ECHONET_FILTER_TXS_FOR_TIMESTAMP_PATH: &str = "/echonet/filter_txs_for_timestamp";

#[derive(Debug, Error)]
pub enum EchonetTxFilterError {
    #[error(transparent)]
    RequestError(#[from] reqwest::Error),
    #[error("Echonet tx filter returned non-200 status: {0}")]
    BadStatus(StatusCode),
    #[error("Echonet tx filter response was missing 'allowed' field")]
    InvalidResponse,
}

pub type EchonetTxFilterResult<T> = Result<T, EchonetTxFilterError>;

#[derive(Clone)]
pub struct EchonetTxFilterClient {
    url: Sensitive<Url>,
    client: reqwest::Client,
    timeout: Duration,
}

#[async_trait]
pub trait EchonetTxFilterClientTrait: Send + Sync {
    async fn allowed_txs_for_timestamp(
        &self,
        timestamp: u64,
        tx_hashes: &[TransactionHash],
    ) -> EchonetTxFilterResult<HashSet<String>>;
}

#[derive(Serialize)]
struct FilterRequest<'a> {
    timestamp: u64,
    tx_hashes: &'a [String],
}

#[derive(Deserialize)]
struct FilterResponse {
    allowed: Option<Vec<String>>,
    // Future fields (optional): deferred / suggested_next_timestamp.
}

impl EchonetTxFilterClient {
    pub fn new(recorder_url: Sensitive<Url>, timeout: Duration) -> Self {
        let url = recorder_url
            .expose_secret()
            .join(ECHONET_FILTER_TXS_FOR_TIMESTAMP_PATH)
            .expect("Failed to construct echonet tx filter URL")
            .into();
        Self { url, client: reqwest::Client::new(), timeout }
    }
}

#[async_trait]
impl EchonetTxFilterClientTrait for EchonetTxFilterClient {
    async fn allowed_txs_for_timestamp(
        &self,
        timestamp: u64,
        tx_hashes: &[TransactionHash],
    ) -> EchonetTxFilterResult<HashSet<String>> {
        let tx_hash_hex: Vec<String> = tx_hashes.iter().map(|h| h.0.to_hex_string()).collect();
        let req = FilterRequest { timestamp, tx_hashes: &tx_hash_hex };

        debug!(
            "Calling echonet tx filter: ts={} n_hashes={}",
            timestamp,
            tx_hash_hex.len()
        );
        let response = self
            .client
            .post(self.url.clone().expose_secret().clone())
            .timeout(self.timeout)
            .json(&req)
            .send()
            .await?;

        if !response.status().is_success() {
            warn!("Echonet tx filter returned status {}", response.status());
            return Err(EchonetTxFilterError::BadStatus(response.status()));
        }

        let body: FilterResponse = response.json().await?;
        let allowed = body.allowed.ok_or(EchonetTxFilterError::InvalidResponse)?;
        Ok(HashSet::from_iter(allowed))
    }
}

