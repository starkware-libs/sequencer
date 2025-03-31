use std::collections::{BTreeMap, HashMap};

use apollo_config::converters::{deserialize_optional_map, serialize_optional_map};
use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_l1_gas_price_types::errors::EthToStrkOracleClientError;
use apollo_l1_gas_price_types::EthToStrkOracleClientTrait;
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json;
use tracing::{debug, info};
use url::Url;

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
    /// The time margin (in seconds) to adjust the timestamp before querying the conversion rate.  
    /// Ensures a stable timestamp from providers. Should be configured in alignment with  
    /// relevant query parameters in `base_url`, if required.
    pub lag_margin_seconds: u64,
}

impl SerializeConfig for EthToStrkOracleConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "base_url",
                &self.base_url,
                "URL to query. This must end with the query parameter `timestamp=` as we append a \
                 UNIX timestamp.",
                ParamPrivacyInput::Private,
            ),
            ser_param(
                "headers",
                &serialize_optional_map(&self.headers),
                "HTTP headers for the eth to strk oracle, formatted as 'k1:v1 k2:v2 ...'.",
                ParamPrivacyInput::Private,
            ),
            ser_param(
                "lag_margin_seconds",
                &self.lag_margin_seconds,
                "The time margin (in seconds) to adjust the timestamp before querying the \
                 conversion rate. Ensures a stable timestamp from providers. Should be configured \
                 in alignment with relevant query parameters in `base_url`, if required.",
                ParamPrivacyInput::Private,
            ),
        ])
    }
}

impl Default for EthToStrkOracleConfig {
    fn default() -> Self {
        Self {
            base_url: Url::parse("https://example.com/api?timestamp=").unwrap(),
            headers: None,
            lag_margin_seconds: 0,
        }
    }
}

/// Client for interacting with the eth to strk Oracle API.
pub struct EthToStrkOracleClient {
    /// The base URL of the eth to strk Oracle API.
    /// This must end with the query parameter `timestamp=` as we append a UNIX timestamp.
    base_url: Url,
    /// HTTP headers required for requests.
    headers: HeaderMap,
    /// The time margin (in seconds) to adjust the timestamp before querying the conversion rate.
    lag_margin_seconds: u64,
    client: reqwest::Client,
}

impl EthToStrkOracleClient {
    pub fn new(
        base_url: Url,
        headers: Option<HashMap<String, String>>,
        lag_margin_seconds: u64,
    ) -> Self {
        info!(
            "Creating EthToStrkOracleClient with: base_url={base_url} headers={headers:?} \
             lag_margin_seconds={lag_margin_seconds}"
        );
        Self {
            base_url,
            headers: hashmap_to_headermap(headers),
            lag_margin_seconds,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl EthToStrkOracleClientTrait for EthToStrkOracleClient {
    /// The HTTP response must include the following fields:
    /// - `"price"`: a hexadecimal string representing the price.
    /// - `"decimals"`: a `u64` value, must be equal to `ETH_TO_STRK_QUANTIZATION`.
    async fn eth_to_fri_rate(&self, timestamp: u64) -> Result<u128, EthToStrkOracleClientError> {
        let adjusted_timestamp = timestamp - self.lag_margin_seconds;
        let url = format!("{}{}", self.base_url, adjusted_timestamp);
        let response = self.client.get(&url).headers(self.headers.clone()).send().await?;
        let body = response.text().await?;

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
        debug!("Conversion rate for timestamp {timestamp} is {rate}");
        Ok(rate)
    }
}
