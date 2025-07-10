use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Mutex;
use std::time::Duration;

use apollo_config::converters::{
    deserialize_optional_list_with_url_and_headers,
    serialize_optional_list_with_url_and_headers,
    UrlAndHeaders,
};
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
    ETH_TO_STRK_RATE,
    ETH_TO_STRK_SUCCESS_COUNT,
};

#[cfg(test)]
#[path = "eth_to_strk_oracle_test.rs"]
pub mod eth_to_strk_oracle_test;

pub const ETH_TO_STRK_QUANTIZATION: u64 = 18;

fn btreemap_to_headermap(hash_map: BTreeMap<String, String>) -> HeaderMap {
    let mut header_map = HeaderMap::new();
    for (key, value) in hash_map {
        header_map.insert(
            HeaderName::from_bytes(key.as_bytes()).expect("Failed to parse header name"),
            HeaderValue::from_str(&value).expect("Failed to parse header value"),
        );
    }
    header_map
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct EthToStrkOracleConfig {
    #[serde(deserialize_with = "deserialize_optional_list_with_url_and_headers")]
    pub url_header_list: Option<Vec<UrlAndHeaders>>,
    pub lag_interval_seconds: u64,
    pub max_cache_size: usize,
    pub query_timeout_sec: u64,
}

impl SerializeConfig for EthToStrkOracleConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "url_header_list",
                &serialize_optional_list_with_url_and_headers(&self.url_header_list),
                "A list of Url+HTTP headers for the eth to strk oracle. \
                 The url is followed by a comma and then headers as key^value pairs, separated by commas. \
                 For example: `https://api.example.com/api,key1^value1,key2^value2`. \
                 Each URL+headers is separated by a pipe `|` character. \
                 The `timestamp` parameter is appended dynamically when making requests, in order \
                 to have a stable mapping from block timestamp to conversion rate. ",
                ParamPrivacyInput::Private,
            ),
            ser_param(
                "lag_interval_seconds",
                &self.lag_interval_seconds,
                "The size of the interval (seconds) that the eth to strk rate is taken on. The \
                 lag refers to the fact that the interval `[T, T+k)` contains the conversion rate \
                 for queries in the interval `[T+k, T+2k)`. Should be configured in alignment \
                 with relevant query parameters in `url_header_list`, if required.",
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
            url_header_list: Some(vec![UrlAndHeaders {
                url: Url::parse("https://api.example.com/api").expect("Invalid URL"),
                headers: BTreeMap::new(),
            }]),
            lag_interval_seconds: 1,
            max_cache_size: 100,
            query_timeout_sec: 3,
        }
    }
}

/// Client for interacting with the eth to strk Oracle API.
pub struct EthToStrkOracleClient {
    config: EthToStrkOracleConfig,
    /// The index of the current URL in the `url_header_list`.
    /// If one URL fails, index is incremented to try the next URL.
    index: usize,
    client: reqwest::Client,
    cached_prices: Mutex<LruCache<u64, u128>>,
    queries: Mutex<LruCache<u64, AbortOnDropHandle<Result<u128, EthToStrkOracleClientError>>>>,
}

impl EthToStrkOracleClient {
    pub fn new(config: EthToStrkOracleConfig) -> Self {
        info!(
            "Creating EthToStrkOracleClient with: url_header_list={:?} lag_interval_seconds={}",
            config.url_header_list, config.lag_interval_seconds
        );
        register_eth_to_strk_metrics();
        Self {
            config: config.clone(),
            index: 0,
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
        // The `timestamp` parameter is appended dynamically to the base URL,
        // in order to have a stable mapping from block timestamp to conversion rate.
        let UrlAndHeaders { url: base_url, headers } = self
            .config
            .url_header_list
            .as_ref()
            .expect("url_header_list should be provided in EthToStrkOracleConfig")
            .get(self.index)
            .unwrap_or_else(|| panic!("cannot find URL and headers for index {}", self.index))
            .clone();
        let headers = btreemap_to_headermap(headers);
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
    ETH_TO_STRK_RATE.set_lossy(rate);
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

        let task_result = handle.now_or_never().expect("Handle must be finished if we got here");
        let query_result = match task_result {
            Ok(query_result) => query_result,
            Err(e) => {
                queries.pop(&quantized_timestamp);
                warn!("Query failed to join handle for timestamp {timestamp}: {e:?}");
                ETH_TO_STRK_ERROR_COUNT.increment(1);
                return Err(EthToStrkOracleClientError::JoinError(e));
            }
        };
        let rate = match query_result {
            Ok(rate) => rate,
            Err(e) => {
                queries.pop(&quantized_timestamp);
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
