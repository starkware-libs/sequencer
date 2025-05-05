use alloy::providers::{Provider, ProviderBuilder};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{error, warn};
use url::Url;

type L1EndpointMonitorResult<T> = Result<T, L1EndpointMonitorError>;
#[derive(Debug, Clone)]
pub struct L1EndpointMonitor {
    pub current_l1_endpoint_index: usize,
    pub config: L1EndpointMonitorConfig,
}

impl L1EndpointMonitor {
    /// Returns a functional L1 endpoint, or fails if all configured endpoints are non-operational.
    /// The method cycles through the configured endpoints, starting from the currently selected one
    /// and returns the first one that is operational.
    pub async fn get_active_l1_endpoint(&mut self) -> L1EndpointMonitorResult<Url> {
        let current_l1_endpoint_index = self.current_l1_endpoint_index;
        // This check can be done async, instead of blocking the user, but this requires an
        // additional "active" component or async task in our infra.
        if self.is_operational(current_l1_endpoint_index).await {
            return Ok(self.get_node_url(current_l1_endpoint_index).clone());
        }

        let n_urls = self.config.ordered_l1_endpoint_urls.len();
        for offset in 1..n_urls {
            let idx = (current_l1_endpoint_index + offset) % n_urls;
            if self.is_operational(idx).await {
                warn!(
                    "L1 endpoint {} down; switched to {}",
                    self.get_node_url(current_l1_endpoint_index),
                    self.get_node_url(idx)
                );

                self.current_l1_endpoint_index = idx;
                return Ok(self.get_node_url(idx).clone());
            }
        }

        error!("No operational L1 endpoints found in {:?}", self.config.ordered_l1_endpoint_urls);
        Err(L1EndpointMonitorError::NoActiveL1Endpoint)
    }

    fn get_node_url(&self, index: usize) -> &Url {
        &self.config.ordered_l1_endpoint_urls[index]
    }

    async fn is_operational(&self, l1_endpoint_index: usize) -> bool {
        let l1_endpoint_url = self.get_node_url(l1_endpoint_index);
        let l1_client = ProviderBuilder::new().on_http(l1_endpoint_url.clone());
        // Is this fast enough? we can use something to just check connectivity, but a recent infura
        // bug failed on this API even though connectivity was fine. Besides, this API is called for
        // most of our operations anyway.
        l1_client.get_block_number().await.is_ok()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct L1EndpointMonitorConfig {
    pub ordered_l1_endpoint_urls: Vec<Url>,
}

#[derive(Debug, Clone, Error, Serialize, Deserialize, PartialEq, Eq)]
pub enum L1EndpointMonitorError {
    #[error("All L1 endpoints are non-operational")]
    NoActiveL1Endpoint,
}
