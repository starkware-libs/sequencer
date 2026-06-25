use std::collections::BTreeMap;
use std::time::Duration;

use apollo_config::converters::{
    deserialize_float_seconds_to_duration,
    deserialize_optional_sensitive_list_with_url_and_headers,
    serialize_duration_as_float_seconds,
    UrlAndHeaders,
};
use apollo_config::secrets::Sensitive;
use apollo_config::validators::validate_ascii;
use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;
use url::Url;
use validator::Validate;

#[cfg(test)]
#[path = "config_test.rs"]
mod config_test;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Validate)]
pub struct ExchangeRateOracleConfig {
    #[serde(deserialize_with = "deserialize_optional_sensitive_list_with_url_and_headers")]
    pub url_header_list: Option<Vec<Sensitive<UrlAndHeaders>>>,
    pub lag_interval_seconds: u64,
    pub max_cache_size: usize,
    pub query_timeout_sec: u64,
}

impl Default for ExchangeRateOracleConfig {
    fn default() -> Self {
        Self {
            url_header_list: Some(vec![
                UrlAndHeaders {
                    url: Url::parse("https://api.example.com/api").expect("Invalid URL"),
                    headers: BTreeMap::new(),
                }
                .into(),
            ]),
            lag_interval_seconds: 1,
            max_cache_size: 100,
            query_timeout_sec: 10,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct L1GasPriceProviderConfig {
    // TODO(guyn): these two fields need to go into VersionedConstants.
    // Must be >= 1: the provider divides the summed prices by this window when computing the mean,
    // so a value of 0 would cause a divide-by-zero panic during block production.
    #[validate(range(min = 1))]
    pub number_of_blocks_for_mean: u64,
    // Use seconds not Duration since seconds is the basic quanta of time for both Starknet and
    // Ethereum.
    #[serde(
        deserialize_with = "deserialize_float_seconds_to_duration",
        serialize_with = "serialize_duration_as_float_seconds"
    )]
    pub lag_margin_seconds: Duration,
    pub storage_limit: usize,
    // Maximum valid time gap between the requested timestamp and the last price sample in seconds.
    pub max_time_gap_seconds: u64,
    #[validate(nested)]
    pub eth_to_strk_oracle_config: ExchangeRateOracleConfig,
    #[validate(nested)]
    pub strk_to_usd_oracle_config: ExchangeRateOracleConfig,
}

impl Default for L1GasPriceProviderConfig {
    fn default() -> Self {
        const MEAN_NUMBER_OF_BLOCKS: u64 = 300;
        Self {
            number_of_blocks_for_mean: MEAN_NUMBER_OF_BLOCKS,
            lag_margin_seconds: Duration::from_secs(60),
            storage_limit: usize::try_from(10 * MEAN_NUMBER_OF_BLOCKS).unwrap(),
            max_time_gap_seconds: 900, // 15 minutes
            eth_to_strk_oracle_config: ExchangeRateOracleConfig::default(),
            strk_to_usd_oracle_config: ExchangeRateOracleConfig::default(),
        }
    }
}

// TODO(guyn): find a way to synchronize the value of number_of_blocks_for_mean
// with the one in L1GasPriceProviderConfig. In the end they should both be loaded
// from VersionedConstants.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct L1GasPriceScraperConfig {
    /// This field is ignored by the L1EventsScraper.
    /// Manual override to specify where the scraper should start.
    /// If None, the node will start scraping from 2*number_of_blocks_for_mean before the tip of
    /// L1.
    pub starting_block: Option<u64>,
    #[validate(custom(function = "validate_ascii"))]
    pub chain_id: ChainId,
    pub finality: u64,
    #[serde(
        deserialize_with = "deserialize_float_seconds_to_duration",
        serialize_with = "serialize_duration_as_float_seconds"
    )]
    pub polling_interval: Duration,
    pub number_of_blocks_for_mean: u64,
    // How many sets of config.num_blocks_for_mean blocks to go back
    // on the chain when starting to scrape.
    pub startup_num_blocks_multiplier: u64,
}

impl Default for L1GasPriceScraperConfig {
    fn default() -> Self {
        Self {
            starting_block: None,
            chain_id: ChainId::Other("0x0".to_string()),
            finality: 0,
            polling_interval: Duration::from_secs(1),
            number_of_blocks_for_mean: 300,
            startup_num_blocks_multiplier: 2,
        }
    }
}
