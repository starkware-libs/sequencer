use std::collections::BTreeMap;

use papyrus_config::converters::deserialize_string_to_btreemap;
use papyrus_config::dumping::{ser_required_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializationType, SerializedParam};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json;
use url::Url;

#[cfg(test)]
#[path = "price_oracle_test.rs"]
pub mod price_oracle_test;

fn btreemap_to_headermap(btree_map: BTreeMap<String, String>) -> HeaderMap {
    let mut header_map = HeaderMap::new();

    for (key, value) in btree_map {
        let header_name = HeaderName::from_bytes(key.as_bytes()).expect("Invalid header name");
        let header_value = HeaderValue::from_str(&value).expect("Invalid header value");

        header_map.insert(header_name, header_value);
    }

    header_map
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct PriceOracleConfig {
    pub base_url: Url,
    #[serde(deserialize_with = "deserialize_string_to_btreemap")]
    pub headers: BTreeMap<String, String>,
}

impl SerializeConfig for PriceOracleConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_required_param(
                "base_url",
                SerializationType::String,
                "The base URL of the Price Oracle API. This must end with 'timestamp=' as the API \
                 requires appending a UNIX timestamp.",
                ParamPrivacyInput::Private,
            ),
            ser_required_param(
                "headers",
                SerializationType::String,
                "HTTP headers required for requests, typically containing authentication details.",
                ParamPrivacyInput::Private,
            ),
        ])
    }
}

impl Default for PriceOracleConfig {
    fn default() -> Self {
        Self {
            base_url: Url::parse("https://example.com/api?timestamp=").unwrap(),
            headers: BTreeMap::new(),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum PriceOracleClientError {
    #[error(transparent)]
    RequestError(#[from] reqwest::Error),
    #[error("Missing or invalid field: {0}")]
    MissingFieldError(&'static str),
}

#[allow(dead_code)]
trait PriceOracleClientTrait {
    /// Fetches the ETH to FRI rate for a given timestamp.
    ///
    /// The response must contain:
    /// - `"price"`: a hexadecimal string
    /// - `"decimals"`: a `u64` value (must be `18`)
    async fn eth_to_fri_rate(&self, timestamp: u64) -> Result<u128, PriceOracleClientError>;
}

/// Client for interacting with the Price Oracle API.
pub struct PriceOracleClient {
    /// The base URL of the Price Oracle API.  
    /// This must end with `"timestamp="` as the API requires appending a UNIX timestamp.
    base_url: Url,
    /// HTTP headers required for requests, typically containing authentication details.
    headers: HeaderMap,
    /// The internal HTTP client.
    client: reqwest::Client,
}

impl PriceOracleClient {
    pub fn new(base_url: Url, headers: BTreeMap<String, String>) -> Self {
        Self { base_url, headers: btreemap_to_headermap(headers), client: reqwest::Client::new() }
    }
}

impl PriceOracleClientTrait for PriceOracleClient {
    async fn eth_to_fri_rate(&self, timestamp: u64) -> Result<u128, PriceOracleClientError> {
        let url = format!("{}{}", self.base_url, timestamp);
        let response = self.client.get(&url).headers(self.headers.clone()).send().await?;
        let body = response.text().await?;

        let json: serde_json::Value = serde_json::from_str(&body).expect("Invalid JSON response");
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
        assert!(decimals == 18);
        Ok(rate)
    }
}
