use std::collections::BTreeMap;
use std::time::Duration;

use alloy::primitives::U64;
use alloy::providers::{Provider, ProviderBuilder};
use apollo_config::converters::{
    deserialize_milliseconds_to_duration,
    deserialize_vec,
    serialize_slice,
};
use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_infra::component_definitions::ComponentStarter;
use apollo_l1_endpoint_monitor_types::{L1EndpointMonitorError, L1EndpointMonitorResult};
use serde::{Deserialize, Serialize};
use tracing::{error, warn};
use url::Url;
use validator::Validate;
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
    pub fn new(
        config: L1EndpointMonitorConfig,
        initial_node_url: &Url,
    ) -> L1EndpointMonitorResult<Self> {
        let starting_l1_endpoint_index =
            config.ordered_l1_endpoint_urls.iter().position(|url| url == initial_node_url).ok_or(
                L1EndpointMonitorError::InitializationError {
                    unknown_url: initial_node_url.clone(),
                },
            )?;

        Ok(Self { current_l1_endpoint_index: starting_l1_endpoint_index, config })
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

        error!(
            "No operational L1 endpoints found in {:?}",
            // We print only the hostnames to avoid leaking the API keys.
            self.config
                .ordered_l1_endpoint_urls
                .iter()
                .map(|url| url
                    .host()
                    .map_or_else(|| "no host in url!".to_string(), |host| host.to_string()))
                .collect::<Vec<_>>()
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
        let l1_client = ProviderBuilder::new().on_http(l1_endpoint_url.clone());

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
            Ok(Ok(_)) => true,
        }
    }
}

impl ComponentStarter for L1EndpointMonitor {}

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq, Eq)]
pub struct L1EndpointMonitorConfig {
    #[serde(deserialize_with = "deserialize_vec")]
    pub ordered_l1_endpoint_urls: Vec<Url>,
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    pub timeout_millis: Duration,
}

impl Default for L1EndpointMonitorConfig {
    fn default() -> Self {
        Self {
            ordered_l1_endpoint_urls: vec![
                Url::parse("https://mainnet.infura.io/v3/YOUR_INFURA_API_KEY").unwrap(),
                Url::parse("https://eth-mainnet.g.alchemy.com/v2/YOUR_ALCHEMY_API_KEY").unwrap(),
            ],
            timeout_millis: Duration::from_millis(1000),
        }
    }
}

impl SerializeConfig for L1EndpointMonitorConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([
            ser_param(
                "ordered_l1_endpoint_urls",
                &serialize_slice(&self.ordered_l1_endpoint_urls),
                "Ordered list of L1 endpoint URLs, used in order, cyclically, switching if the \
                 current one is non-operational.",
                ParamPrivacyInput::Private,
            ),
            ser_param(
                "timeout_millis",
                &self.timeout_millis.as_millis(),
                "The timeout (milliseconds) for a query of the L1 base layer",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}
