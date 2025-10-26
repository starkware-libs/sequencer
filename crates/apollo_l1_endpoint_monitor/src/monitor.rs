use alloy::primitives::U64;
use alloy::providers::{Provider, ProviderBuilder};
use apollo_infra::component_definitions::ComponentStarter;
use apollo_infra_utils::info_every_n;
use apollo_l1_endpoint_monitor_config::config::L1EndpointMonitorConfig;
use apollo_l1_endpoint_monitor_types::{L1EndpointMonitorError, L1EndpointMonitorResult};
use tracing::{error, warn};
use url::Url;
#[cfg(test)]
#[path = "l1_endpoint_monitor_tests.rs"]
pub mod l1_endpoint_monitor_tests;

/// The JSON-RPC method used to check L1 endpoint health.
// Note: is this fast enough? Alternatively, we can just check connectivity, but we already hit
// a bug in infura where the connectivity was fine, but get_block_number() failed.
pub const HEALTH_CHECK_RPC_METHOD: &str = "eth_blockNumber";

/// The minimum expected L1 block number for a valid endpoint response.
pub const MIN_EXPECTED_BLOCK_NUMBER: u64 = 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct L1EndpointMonitor {
    pub current_l1_endpoint_index: usize,
    pub config: L1EndpointMonitorConfig,
}

impl L1EndpointMonitor {
    pub fn new(config: L1EndpointMonitorConfig) -> Self {
        Self { current_l1_endpoint_index: 0, config }
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
                    to_safe_string(self.get_node_url(current_l1_endpoint_index)),
                    to_safe_string(self.get_node_url(idx))
                );

                self.current_l1_endpoint_index = idx;
                return Ok(self.get_node_url(idx).clone());
            }
        }

        error!(
            "No operational L1 endpoints found in {:?}",
            // We print only the hostnames to avoid leaking the API keys.
            self.config.ordered_l1_endpoint_urls.iter().map(to_safe_string).collect::<Vec<_>>()
        );
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
        let l1_client = ProviderBuilder::new().connect_http(l1_endpoint_url.clone());
        let l1_endpoint_url = to_safe_string(l1_endpoint_url);

        // Note: response type annotation is coupled with the rpc method used.
        let is_operational_result = tokio::time::timeout(
            self.config.timeout_millis,
            l1_client.client().request_noparams::<U64>(HEALTH_CHECK_RPC_METHOD),
        )
        .await;

        match is_operational_result {
            Err(_) => {
                error!("timed-out while testing L1 endpoint {l1_endpoint_url}");
                false
            }
            Ok(Err(e)) => {
                error!("L1 endpoint {l1_endpoint_url} is not operational: {e}");
                false
            }
            Ok(Ok(block_number)) => {
                // TODO(guyn): remove this once we understand where these low numbers are coming
                // from.
                if block_number < U64::from(MIN_EXPECTED_BLOCK_NUMBER) {
                    warn!(
                        "L1 endpoint {l1_endpoint_url} is operational, but block number is too \
                         low: {block_number}"
                    );
                }

                info_every_n!(1000, "L1 endpoint {l1_endpoint_url} is operational");
                true
            }
        }
    }
}

impl ComponentStarter for L1EndpointMonitor {}

// TODO(Arni): Move to apollo_infra_utils.
fn to_safe_string(url: &Url) -> String {
    // We print only the hostnames to avoid leaking the API keys.
    url.host().map_or_else(|| "no host in url!".to_string(), |host| host.to_string())
}
