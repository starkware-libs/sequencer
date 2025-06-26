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
use futures::FutureExt;
use lru::LruCache;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json;
use tokio_util::task::AbortOnDropHandle;
use tracing::{debug, info, instrument, warn};
use url::Url;

use crate::metrics::{
    register_eth_to_strk_metrics,
    ETH_TO_STRK_ERROR_COUNT,
    ETH_TO_STRK_SUCCESS_COUNT,
};

#[cfg(test)]
#[path = "eth_to_strk_oracle_test.rs"]
pub mod eth_to_strk_oracle_test;

pub const ETH_TO_STRK_QUANTIZATION: u64 = 18;

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
    pub max_cache_size: usize,
    pub query_timeout_sec: u64,
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
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_cache_size",
                &self.max_cache_size,
                "The maximum number of cached conversion rates.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "query_timeout_sec",
                &self.query_timeout_sec,
                "The timeout (seconds) for the query to the eth to strk oracle.",
                ParamPrivacyInput::Public,
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
            max_cache_size: 100,
            query_timeout_sec: 3,
        }
    }
}

/// Client for interacting with the eth to strk Oracle API.
pub struct EthToStrkOracleClient {
    config: EthToStrkOracleConfig,
    /// The base URL of the eth to strk Oracle API.
    /// The `timestamp` parameter is appended dynamically when making requests,
    /// in order to have a stable mapping from block timestamp to conversion rate.
    base_url: Url,
    /// HTTP headers required for requests.
    headers: HeaderMap,
    client: reqwest::Client,
    cached_prices: Mutex<LruCache<u64, u128>>,
    queries: Mutex<LruCache<u64, AbortOnDropHandle<Result<u128, EthToStrkOracleClientError>>>>,
}

impl EthToStrkOracleClient {
    pub fn new(config: EthToStrkOracleConfig) -> Self {
        info!(
            "Creating EthToStrkOracleClient with: base_url={:} headers={:?} \
             lag_interval_seconds={}",
            config.base_url, config.headers, config.lag_interval_seconds
        );
        register_eth_to_strk_metrics();
        Self {
            config: config.clone(),
            base_url: config.base_url,
            headers: hashmap_to_headermap(config.headers),
            client: reqwest::Client::new(),
            cached_prices: Mutex::new(LruCache::new(
                NonZeroUsize::new(config.max_cache_size).expect("Invalid cache size"),
            )),
            queries: Mutex::new(LruCache::new(
                NonZeroUsize::new(config.max_cache_size).expect("Invalid cache size"),
            )),
        }
    }

    fn spawn_query(
        &self,
        quantized_timestamp: u64,
    ) -> AbortOnDropHandle<Result<u128, EthToStrkOracleClientError>> {
        let adjusted_timestamp = quantized_timestamp * self.config.lag_interval_seconds;
        let client = self.client.clone();
        let base_url = self.base_url.clone();
        let headers = self.headers.clone();
        let query_timeout_sec = self.config.query_timeout_sec;

        let future = async move {
            let response_body = loop {
                let mut url = base_url.clone();
                url.query_pairs_mut().append_pair("timestamp", &adjusted_timestamp.to_string());

                let result = tokio::time::timeout(Duration::from_secs(query_timeout_sec), async {
                    let response = client.get(url).headers(headers.clone()).send().await?;
                    let body = response.text().await?;
                    Ok::<_, EthToStrkOracleClientError>(body)
                })
                .await;

                match result {
                    Ok(inner_result) => {
                        break inner_result?;
                    }
                    Err(_) => {
                        ETH_TO_STRK_ERROR_COUNT.increment(1);
                        warn!("Timeout when resolving query for timestamp {adjusted_timestamp}");
                        continue;
                    }
                }
            };
            resolve_query(response_body)
        };

        AbortOnDropHandle::new(tokio::spawn(future))
    }
}

fn resolve_query(body: String) -> Result<u128, EthToStrkOracleClientError> {
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
    ETH_TO_STRK_SUCCESS_COUNT.increment(1);
    Ok(rate)
}

#[async_trait]
impl EthToStrkOracleClientTrait for EthToStrkOracleClient {
    /// The HTTP response must include the following fields:
    /// - `price`: a hexadecimal string representing the price.
    /// - `decimals`: a `u64` value, must be equal to `ETH_TO_STRK_QUANTIZATION`.
    #[instrument(skip(self))]
    async fn eth_to_fri_rate(&self, timestamp: u64) -> Result<u128, EthToStrkOracleClientError> {
        let quantized_timestamp = (timestamp - self.config.lag_interval_seconds)
            .checked_div(self.config.lag_interval_seconds)
            .expect("lag_interval_seconds should be non-zero");

        let mut cache = self.cached_prices.lock().unwrap();

        if let Some(rate) = cache.get(&quantized_timestamp) {
            debug!("Cached conversion rate for timestamp {timestamp} is {rate}");
            return Ok(*rate);
        }

        // Check if there is a query already sent out for this timestamp, if not, start one.
        let mut queries = self.queries.lock().unwrap();
        let handle = queries
            .get_or_insert_mut(quantized_timestamp, || self.spawn_query(quantized_timestamp));

        // If the query is not finished, return an error.
        if !handle.is_finished() {
            warn!("Query not yet resolved: timestamp={timestamp}");
            return Err(EthToStrkOracleClientError::QueryNotReadyError(timestamp));
        }

        // TODO(guyn): if we don't care about the warn! we can just use two ?? as follows:
        // let rate = handle.now_or_never().expect("Handle must be finished if we got here")??;
        let outer_result = handle.now_or_never().expect("Handle must be finished if we got here");
        let inner_result = match outer_result {
            Ok(inner_result) => inner_result,
            Err(e) => {
                warn!("Query failed to join handle for timestamp {timestamp}: {e:?}");
                ETH_TO_STRK_ERROR_COUNT.increment(1);
                return Err(EthToStrkOracleClientError::JoinError(e));
            }
        };
        let rate = match inner_result {
            Ok(rate) => rate,
            Err(e) => {
                warn!("Query failed to reach oracle for timestamp {timestamp}: {e:?}");
                ETH_TO_STRK_ERROR_COUNT.increment(1);
                return Err(e);
            }
        };

        // Make sure to cache the result.
        cache.put(quantized_timestamp, rate);
        debug!("Conversion rate for timestamp {timestamp} is {rate}");
        Ok(rate)
    }
}
