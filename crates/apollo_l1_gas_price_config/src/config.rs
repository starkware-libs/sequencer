use std::collections::BTreeMap;
use std::time::Duration;

use apollo_config::converters::{
    deserialize_float_seconds_to_duration,
    deserialize_optional_sensitive_list_with_url_and_headers,
    serialize_optional_list_with_url_and_headers,
    UrlAndHeaders,
};
use apollo_config::dumping::{
    prepend_sub_config_name,
    ser_optional_param,
    ser_param,
    SerializeConfig,
};
use apollo_config::secrets::Sensitive;
use apollo_config::validators::{create_validation_error, validate_ascii};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_l1_gas_price_types::CurrencyPair;
use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;
use url::Url;
use validator::{Validate, ValidationError};

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

impl SerializeConfig for ExchangeRateOracleConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "url_header_list",
                &serialize_optional_list_with_url_and_headers(
                    &self.url_header_list.as_ref().map(|list| {
                        list.iter().map(|s| s.peek_secret()).cloned().collect()
                    }),
                ),
                "A list of Url+HTTP headers for the exchange rate oracle. \
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
                "The size of the interval (seconds) that the exchange rate is taken on. The \
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
                "The timeout (seconds) for the query to the exchange rate oracle.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
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

/// Decimals of the micro-unit rate bounds.
pub const RATE_MICRO_UNIT_DECIMALS: u32 = 6;

/// Inclusive absolute bounds on a rate, in micro units (1e-6) of the pair's quote currency,
/// together with the pair they bound.
// [Temporary comment] No reader yet: the guards arrive in A7.
#[derive(Clone, Copy, Debug)]
pub struct RateBounds {
    pub minimum_micro_units: u64,
    pub maximum_micro_units: u64,
    pub pair: CurrencyPair,
}

/// One pair's bounds, as an operator sets them.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Validate)]
pub struct RateBoundsConfig {
    pub minimum_micro_units: u64,
    pub maximum_micro_units: u64,
}

impl RateBoundsConfig {
    fn bounds(&self, pair: CurrencyPair) -> RateBounds {
        RateBounds {
            minimum_micro_units: self.minimum_micro_units,
            maximum_micro_units: self.maximum_micro_units,
            pair,
        }
    }
}

impl SerializeConfig for RateBoundsConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "minimum_micro_units",
                &self.minimum_micro_units,
                "Lowest accepted rate for this pair, in micro units (1e-6) of the quote currency, \
                 so a value of 20000000 on ETH/USD means $20.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "maximum_micro_units",
                &self.maximum_micro_units,
                "Highest accepted rate for this pair, in micro units (1e-6) of the quote \
                 currency, so a value of 50000000000 on ETH/USD means $50,000.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

/// Absolute bounds every exchange rate must fall in, whichever source reports it.
// [Temporary comment] No reader yet: A7 checks rates, B2 nests this in `L1GasPriceProviderConfig`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Validate)]
#[validate(schema(function = "validate_all_rate_bounds_config"))]
pub struct AllRateBoundsConfig {
    /// Micro-USD per ETH.
    #[validate(nested)]
    pub eth_usd: RateBoundsConfig,
    /// Micro-USD per STRK.
    #[validate(nested)]
    pub strk_usd: RateBoundsConfig,
    /// Micro-STRK per ETH.
    #[validate(nested)]
    pub eth_strk: RateBoundsConfig,
}

impl AllRateBoundsConfig {
    pub fn eth_usd_bounds(&self) -> RateBounds {
        self.eth_usd.bounds(CurrencyPair::EthUsd)
    }

    pub fn strk_usd_bounds(&self) -> RateBounds {
        self.strk_usd.bounds(CurrencyPair::StrkUsd)
    }

    pub fn eth_strk_bounds(&self) -> RateBounds {
        self.eth_strk.bounds(CurrencyPair::EthStrk)
    }
}

impl Default for AllRateBoundsConfig {
    fn default() -> Self {
        const MICRO_UNITS_PER_UNIT: u64 = 10u64.pow(RATE_MICRO_UNIT_DECIMALS);

        Self {
            // $20 .. $50,000 per ETH, ~10x above the all-time high.
            eth_usd: RateBoundsConfig {
                minimum_micro_units: 20 * MICRO_UNITS_PER_UNIT,
                maximum_micro_units: 50_000 * MICRO_UNITS_PER_UNIT,
            },
            // $0.0001 .. $10 per STRK.
            strk_usd: RateBoundsConfig {
                minimum_micro_units: MICRO_UNITS_PER_UNIT / 10_000,
                maximum_micro_units: 10 * MICRO_UNITS_PER_UNIT,
            },
            // 10,000 .. 1,000,000 STRK per ETH, roughly 10x either side of spot near 8.2e4.
            eth_strk: RateBoundsConfig {
                minimum_micro_units: 10_000 * MICRO_UNITS_PER_UNIT,
                maximum_micro_units: 1_000_000 * MICRO_UNITS_PER_UNIT,
            },
        }
    }
}

/// The config key a pair's bounds live under.
fn bounds_config_key(pair: CurrencyPair) -> &'static str {
    match pair {
        CurrencyPair::EthUsd => "eth_usd",
        CurrencyPair::StrkUsd => "strk_usd",
        CurrencyPair::EthStrk => "eth_strk",
    }
}

/// Cross-field checks the per-field `range` attributes cannot express.
fn validate_all_rate_bounds_config(config: &AllRateBoundsConfig) -> Result<(), ValidationError> {
    for bounds in [config.eth_usd_bounds(), config.strk_usd_bounds(), config.eth_strk_bounds()] {
        let pair_name = bounds_config_key(bounds.pair);
        if bounds.minimum_micro_units == 0 {
            return Err(create_validation_error(
                format!("{pair_name}.minimum_micro_units is zero"),
                "zero sanity bound",
                "A zero minimum disables the lower sanity bound; set it to the lowest plausible \
                 value.",
            ));
        }
        if bounds.minimum_micro_units >= bounds.maximum_micro_units {
            return Err(create_validation_error(
                format!(
                    "{pair_name}.minimum_micro_units ({}) is not below \
                     {pair_name}.maximum_micro_units ({})",
                    bounds.minimum_micro_units, bounds.maximum_micro_units
                ),
                "inverted sanity bounds",
                "Ensure each minimum sanity bound is strictly below its maximum.",
            ));
        }
    }
    Ok(())
}

impl SerializeConfig for AllRateBoundsConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = prepend_sub_config_name(self.eth_usd.dump(), "eth_usd");
        config.extend(prepend_sub_config_name(self.strk_usd.dump(), "strk_usd"));
        config.extend(prepend_sub_config_name(self.eth_strk.dump(), "eth_strk"));
        config
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
    #[serde(deserialize_with = "deserialize_float_seconds_to_duration")]
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

impl SerializeConfig for L1GasPriceProviderConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = BTreeMap::from([
            ser_param(
                "number_of_blocks_for_mean",
                &self.number_of_blocks_for_mean,
                "Number of blocks to use for the mean gas price calculation",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "lag_margin_seconds",
                &self.lag_margin_seconds.as_secs(),
                "Difference between the time of the block from L1 used to calculate the gas price \
                 and the time of the L2 block this price is used in",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "storage_limit",
                &self.storage_limit,
                "Maximum number of L1 blocks to keep cached",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_time_gap_seconds",
                &self.max_time_gap_seconds,
                "Maximum valid time gap between the requested timestamp and the last price sample \
                 in seconds",
                ParamPrivacyInput::Public,
            ),
        ]);
        config.extend(prepend_sub_config_name(
            self.eth_to_strk_oracle_config.dump(),
            "eth_to_strk_oracle_config",
        ));
        config.extend(prepend_sub_config_name(
            self.strk_to_usd_oracle_config.dump(),
            "strk_to_usd_oracle_config",
        ));
        config
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
    #[serde(deserialize_with = "deserialize_float_seconds_to_duration")]
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

impl SerializeConfig for L1GasPriceScraperConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = BTreeMap::from([
            ser_param(
                "chain_id",
                &self.chain_id,
                "The chain to follow. For more details see https://docs.starknet.io/documentation/architecture_and_concepts/Blocks/transactions/#chain-id",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "finality",
                &self.finality,
                "Number of blocks to wait for finality in L1",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "polling_interval",
                &self.polling_interval.as_secs(),
                "The duration (seconds) between each scraping attempt of L1",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "number_of_blocks_for_mean",
                &self.number_of_blocks_for_mean,
                "Number of blocks to use for the mean gas price calculation",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "startup_num_blocks_multiplier",
                &self.startup_num_blocks_multiplier,
                "How many sets of config.num_blocks_for_mean blocks to go back on the chain when starting to scrape.",
                ParamPrivacyInput::Public,
            ),
        ]);
        config.extend(ser_optional_param(
            &self.starting_block,
            0, // This value is never used, since #is_none turns it to a None.
            "starting_block",
            "Starting block to scrape from",
            ParamPrivacyInput::Public,
        ));
        config
    }
}
