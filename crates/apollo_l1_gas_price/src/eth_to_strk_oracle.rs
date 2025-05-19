use std::collections::{BTreeMap, HashMap};
use std::num::NonZeroUsize;
use std::sync::Mutex;
use std::time::Duration;

use apollo_config::converters::{deserialize_optional_map, serialize_optional_map};
use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_l1_gas_price_types::errors::EthToStrkOracleClientError;
use apollo_l1_gas_price_types::EthToStrkOracleClientTrait;
use async_trait::async_trait;
use lru::LruCache;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json;
use tokio_util::task::AbortOnDropHandle;
use tracing::{debug, info, instrument, warn};
use url::Url;

#[cfg(test)]
#[path = "eth_to_strk_oracle_test.rs"]
pub mod eth_to_strk_oracle_test;

// TODO(Asmaa): Move to config.
pub const ETH_TO_STRK_QUANTIZATION: u64 = 18;
const MAX_CACHE_SIZE: NonZeroUsize = NonZeroUsize::new(100).expect("Invalid cache size");
const QUERY_TIMEOUT_SEC: u64 = 3;

pub enum Query {
    Resolved(u128),
    Unresolved(AbortOnDropHandle<Result<String, EthToStrkOracleClientError>>),
}

fn hashmap_to_headermap(hash_map: Option<HashMap<String, String>>) -> HeaderMap {
    let mut header_map = HeaderMap::new();
    if let Some(map) = hash_map {
        for (key, value) in map {
            header_map.insert(
                HeaderName::from_bytes(key.as_bytes()).expect("Failed to parse header name"),
                HeaderValue::from_str(&value).expect("Failed to parse header value"),
            );
        }
    }
    header_map
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct EthToStrkOracleConfig {
    pub base_url: Url,
    #[serde(deserialize_with = "deserialize_optional_map")]
    pub headers: Option<HashMap<String, String>>,
    pub lag_interval_seconds: u64,
}

impl SerializeConfig for EthToStrkOracleConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "base_url",
                &self.base_url,
                "URL to query. The `timestamp` parameter is appended dynamically when making \
                 requests, in order to have a stable mapping from block timestamp to conversion \
                 rate.",
                ParamPrivacyInput::Private,
            ),
            ser_param(
                "headers",
                &serialize_optional_map(&self.headers),
                "HTTP headers for the eth to strk oracle, formatted as 'k1:v1 k2:v2 ...'.",
                ParamPrivacyInput::Private,
            ),
            ser_param(
                "lag_interval_seconds",
                &self.lag_interval_seconds,
                "The size of the interval (seconds) that the eth to strk rate is taken on. The \
                 lag refers to the fact that the interval `[T, T+k)` contains the conversion rate \
                 for queries in the interval `[T+k, T+2k)`. Should be configured in alignment \
                 with relevant query parameters in `base_url`, if required.",
                ParamPrivacyInput::Private,
            ),
        ])
    }
}

impl Default for EthToStrkOracleConfig {
    fn default() -> Self {
        Self {
            base_url: Url::parse("https://example.com/api").unwrap(),
            headers: None,
            lag_interval_seconds: 1,
        }
    }
}

/// Client for interacting with the eth to strk Oracle API.
pub struct EthToStrkOracleClient {
    /// The base URL of the eth to strk Oracle API.
    /// The `timestamp` parameter is appended dynamically when making requests,
    /// in order to have a stable mapping from block timestamp to conversion rate.
    base_url: Url,
    /// HTTP headers required for requests.
    headers: HeaderMap,
    lag_interval_seconds: u64,
    client: reqwest::Client,
    cached_prices: Mutex<LruCache<u64, Query>>,
}

impl EthToStrkOracleClient {
    pub fn new(
        base_url: Url,
        headers: Option<HashMap<String, String>>,
        lag_interval_seconds: u64,
    ) -> Self {
        info!(
            "Creating EthToStrkOracleClient with: base_url={base_url} headers={headers:?} \
             lag_interval_seconds={lag_interval_seconds}"
        );
        Self {
            base_url,
            headers: hashmap_to_headermap(headers),
            lag_interval_seconds,
            client: reqwest::Client::new(),
            cached_prices: Mutex::new(LruCache::new(MAX_CACHE_SIZE)),
        }
    }

    fn spawn_query(
        &self,
        quantized_timestamp: u64,
    ) -> AbortOnDropHandle<Result<String, EthToStrkOracleClientError>> {
        let adjusted_timestamp = quantized_timestamp * self.lag_interval_seconds;
        let mut url = self.base_url.clone();
        url.query_pairs_mut().append_pair("timestamp", &adjusted_timestamp.to_string());

        let client = self.client.clone();
        let headers = self.headers.clone();

        let future = async move {
            let response_result =
                tokio::time::timeout(Duration::from_secs(QUERY_TIMEOUT_SEC), async {
                    let response = client.get(url).headers(headers).send().await?;
                    let body = response.text().await?;
                    Ok::<_, EthToStrkOracleClientError>(body)
                })
                .await;

            match response_result {
                Ok(inner) => inner,
                Err(_) => Err(EthToStrkOracleClientError::RequestTimeoutError(adjusted_timestamp)),
            }
        };

        AbortOnDropHandle::new(tokio::spawn(future))
    }

    async fn resolve_query(
        &self,
        quantized_timestamp: u64,
    ) -> Result<u128, EthToStrkOracleClientError> {
        let Some(Query::Unresolved(handle)) = self
            .cached_prices
            .lock()
            .expect("Lock on cached prices was poisoned due to a previous panic")
            .pop(&quantized_timestamp)
        else {
            panic!("Entry must exist")
        };
        assert!(handle.is_finished(), "Should only be called once the query completes");
        let body = handle.await??;
        let json: serde_json::Value = serde_json::from_str(&body)?;
        let price = json
            .get("price")
            .and_then(|v| v.as_str())
            .ok_or(EthToStrkOracleClientError::MissingFieldError("price"))?;
        // Convert hex to u128
        let rate = u128::from_str_radix(price.trim_start_matches("0x"), 16)
            .expect("Failed to parse price as u128");
        // Extract decimals from API response
        let decimals = json
            .get("decimals")
            .and_then(|v| v.as_u64())
            .ok_or(EthToStrkOracleClientError::MissingFieldError("decimals"))?;
        if decimals != ETH_TO_STRK_QUANTIZATION {
            return Err(EthToStrkOracleClientError::InvalidDecimalsError(
                ETH_TO_STRK_QUANTIZATION,
                decimals,
            ));
        }
        Ok(rate)
    }
}

#[async_trait]
impl EthToStrkOracleClientTrait for EthToStrkOracleClient {
    /// The HTTP response must include the following fields:
    /// - `price`: a hexadecimal string representing the price.
    /// - `decimals`: a `u64` value, must be equal to `ETH_TO_STRK_QUANTIZATION`.
    #[instrument(skip(self), err)]
    async fn eth_to_fri_rate(&self, timestamp: u64) -> Result<u128, EthToStrkOracleClientError> {
        let quantized_timestamp = (timestamp - self.lag_interval_seconds)
            .checked_div(self.lag_interval_seconds)
            .expect("lag_interval_seconds should be non-zero");

        // Scope is to make sure the MutexGuard is dropped before the await.
        {
            let mut cached_prices = self.cached_prices.lock().expect("Lock poisoned");
            let Some(query) = cached_prices.get_mut(&quantized_timestamp) else {
                cached_prices.push(
                    quantized_timestamp,
                    Query::Unresolved(self.spawn_query(quantized_timestamp)),
                );
                return Err(EthToStrkOracleClientError::QueryNotReadyError(timestamp));
            };

            match query {
                Query::Resolved(rate) => {
                    debug!("Cached conversion rate for timestamp {timestamp} is {rate}");
                    return Ok(*rate);
                }
                Query::Unresolved(handle) => {
                    if !handle.is_finished() {
                        return Err(EthToStrkOracleClientError::QueryNotReadyError(timestamp));
                    }
                }
            };
        }
        loop {
            match tokio::time::timeout(
                Duration::from_secs(QUERY_TIMEOUT_SEC),
                self.resolve_query(quantized_timestamp),
            )
            .await
            {
                Ok(Ok(rate)) => {
                    self.cached_prices
                        .lock()
                        .expect("Lock on cached prices was poisoned due to a previous panic")
                        .push(quantized_timestamp, Query::Resolved(rate));
                    debug!("Conversion rate for timestamp {timestamp} is {rate}");
                    return Ok(rate);
                }
                Ok(Err(e)) => {
                    return Err(e);
                }
                Err(_) => {
                    warn!("Timeout when resolving query for timestamp {timestamp}");
                }
            }
        }
    }
}
