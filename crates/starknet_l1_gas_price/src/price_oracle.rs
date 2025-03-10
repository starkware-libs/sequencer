use std::collections::HashMap;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json;
use url::Url;

#[cfg(test)]
#[path = "price_oracle_test.rs"]
pub mod price_oracle_test;

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

#[derive(thiserror::Error, Debug)]
pub enum PriceOracleClientError {
    #[error(transparent)]
    RequestError(#[from] reqwest::Error),
    #[error(transparent)]
    ParseError(#[from] serde_json::Error),
    #[error("Missing or invalid field: {0}")]
    MissingFieldError(&'static str),
    #[error("Invalid decimals value: expected 18, got {0}")]
    InvalidDecimalsError(u64),
}

#[allow(dead_code)]
trait PriceOracleClientTrait {
    /// Fetches the ETH to FRI rate for a given timestamp.
    ///
    /// The HTTP response must include the following fields:
    /// - `"price"`: a hexadecimal string representing the price.
    /// - `"decimals"`: a `u64` value, must be `18`.
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
    pub fn new(base_url: Url, headers: Option<HashMap<String, String>>) -> Self {
        Self { base_url, headers: hashmap_to_headermap(headers), client: reqwest::Client::new() }
    }
}

impl PriceOracleClientTrait for PriceOracleClient {
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
        if decimals != 18 {
            return Err(PriceOracleClientError::InvalidDecimalsError(decimals));
        }
        Ok(rate)
    }
}
