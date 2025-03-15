use std::collections::{BTreeMap, HashMap};

use async_trait::async_trait;
use papyrus_config::converters::{deserialize_optional_map, serialize_optional_map};
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json;
use starknet_l1_gas_price_types::errors::PriceOracleClientError;
use starknet_l1_gas_price_types::PriceOracleClientTrait;
use url::Url;

#[cfg(test)]
#[path = "price_oracle_test.rs"]
pub mod price_oracle_test;

const DECIMALS: u64 = 18;

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
pub struct PriceOracleConfig {
    pub base_url: Url,
    #[serde(deserialize_with = "deserialize_optional_map")]
    pub headers: Option<HashMap<String, String>>,
}

impl SerializeConfig for PriceOracleConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "base_url",
                &self.base_url,
                "This must end with the query parameter `timestamp=` as we append a UNIX \
                 timestamp.",
                ParamPrivacyInput::Private,
            ),
            ser_param(
                "headers",
                &serialize_optional_map(&self.headers),
                "Headers for the price oracle client, formatted as 'k1:v1 k2:v2 ...'.",
                ParamPrivacyInput::Private,
            ),
        ])
    }
}

impl Default for PriceOracleConfig {
    fn default() -> Self {
        Self { base_url: Url::parse("https://example.com/api?timestamp=").unwrap(), headers: None }
    }
}

/// Client for interacting with the Price Oracle API.
pub struct PriceOracleClient {
    /// The base URL of the Price Oracle API.
    /// This must end with the query parameter `timestamp=` as we append a UNIX timestamp.
    base_url: Url,
    /// HTTP headers required for requests.
    headers: HeaderMap,
    client: reqwest::Client,
}

impl PriceOracleClient {
    pub fn new(base_url: Url, headers: Option<HashMap<String, String>>) -> Self {
        Self { base_url, headers: hashmap_to_headermap(headers), client: reqwest::Client::new() }
    }
}

#[async_trait]
impl PriceOracleClientTrait for PriceOracleClient {
    /// The HTTP response must include the following fields:
    /// - `"price"`: a hexadecimal string representing the price.
    /// - `"decimals"`: a `u64` value, must be equal to `DECIMALS`.
    async fn eth_to_fri_rate(&self, timestamp: u64) -> Result<u128, PriceOracleClientError> {
        let url = format!("{}{}", self.base_url, timestamp);
        let response = self.client.get(&url).headers(self.headers.clone()).send().await?;
        let body = response.text().await?;

        let json: serde_json::Value = serde_json::from_str(&body)?;
        let price = json
            .get("price")
            .and_then(|v| v.as_str())
            .ok_or(PriceOracleClientError::MissingFieldError("price"))?;
        // Convert hex to u128
        let rate = u128::from_str_radix(price.trim_start_matches("0x"), 16)
            .expect("Failed to parse price as u128");
        // Extract decimals from API response
        let decimals = json
            .get("decimals")
            .and_then(|v| v.as_u64())
            .ok_or(PriceOracleClientError::MissingFieldError("decimals"))?;
        if decimals != DECIMALS {
            return Err(PriceOracleClientError::InvalidDecimalsError(DECIMALS, decimals));
        }
        Ok(rate)
    }
}
