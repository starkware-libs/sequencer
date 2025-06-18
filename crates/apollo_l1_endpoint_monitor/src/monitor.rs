use alloy::primitives::U64;
use alloy::providers::{Provider, ProviderBuilder};
use apollo_l1_endpoint_monitor_types::{L1EndpointMonitorError, L1EndpointMonitorResult};
use serde::{Deserialize, Serialize};
use tracing::{error, warn};
use url::Url;

#[cfg(test)]
#[path = "l1_endpoint_monitor_tests.rs"]
pub mod l1_endpoint_monitor_tests;

/// The JSON-RPC method used to check L1 endpoint health.
// Note: is this fast enough? Alternatively, we can just check connectivity, but we already hit
// a bug in infura where the connectivity was fine, but get_block_number() failed.
pub const HEALTH_CHECK_RPC_METHOD: &str = "eth_blockNumber";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct L1EndpointMonitor {
    pub current_l1_endpoint_index: usize,
    pub config: L1EndpointMonitorConfig,
}

impl L1EndpointMonitor {
    /// Initializes the l1 endpoint monitor to the first endpoint in the config's endpoint list.
    pub fn new(config: L1EndpointMonitorConfig) -> L1EndpointMonitorResult<Self> {
        if config.ordered_l1_endpoint_urls.is_empty() {
            return Err(L1EndpointMonitorError::InitializationError);
        }

        Ok(Self { current_l1_endpoint_index: 0, config })
    }

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

    /// Check if the L1 endpoint is operational by sending a carefully-chosen request to it.
    // note: Using a raw request instead of just alloy API (like `get_block_number()`) to improve
    // high-level readability (through a dedicated const) and to improve testability.
    async fn is_operational(&self, l1_endpoint_index: usize) -> bool {
        let l1_endpoint_url = self.get_node_url(l1_endpoint_index);
        let l1_client = ProviderBuilder::new().on_http(l1_endpoint_url.clone());
        // Note: response type annotation is coupled with the rpc method used.
        l1_client.client().request_noparams::<U64>(HEALTH_CHECK_RPC_METHOD).await.is_ok()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct L1EndpointMonitorConfig {
    pub ordered_l1_endpoint_urls: Vec<Url>,
}
