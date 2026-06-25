use std::collections::BTreeMap;
use std::time::Duration;

use apollo_config::converters::{
    deserialize_float_seconds_to_duration,
    deserialize_optional_sensitive_list_with_url_and_headers,
    serialize_duration_as_float_seconds,
    UrlAndHeaders,
};
use apollo_config::secrets::Sensitive;
use apollo_config::validators::{create_validation_error, validate_ascii};
use apollo_l1_gas_price_types::{CurrencyPair, ExchangeRate, EXCHANGE_RATE_DECIMALS};
use serde::{Deserialize, Serialize};
use starknet_api::core::{ChainId, ContractAddress};
use starknet_types_core::felt::Felt;
use url::Url;
use validator::{Validate, ValidationError};

#[cfg(test)]
#[path = "config_test.rs"]
mod config_test;

/// Which implementation serves a single exchange rate feed. The two feeds are selected
/// independently, so they can be migrated one at a time.
// TODO(Asaf): remove this enum, both `*_oracle_source` fields and both their params together
// with the HTTP oracle, once both feeds have run on `Chainlink` on mainnet for a week.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum ExchangeRateOracleSource {
    /// The off-chain oracle HTTP API, configured by the feed's `ExchangeRateOracleConfig`.
    #[default]
    Http,
    /// Chainlink's on-chain Starknet price feeds, configured by `ChainlinkOracleConfig` and read
    /// through the batcher.
    Chainlink,
}

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

/// Decimals of the micro-unit rate bounds an operator sets.
pub const RATE_MICRO_UNIT_DECIMALS: u32 = 6;

/// Scale factor from a micro-unit bound to an `ExchangeRate`.
const MICRO_UNIT_TO_RATE_SCALE: ExchangeRate =
    10u128.pow(EXCHANGE_RATE_DECIMALS - RATE_MICRO_UNIT_DECIMALS);

/// Inclusive absolute bounds on a rate, at `EXCHANGE_RATE_DECIMALS`, together with the pair they
/// bound. Scaled up from the operator's micro units once per read, so every comparison against a
/// rate is a direct one.
#[derive(Clone, Copy, Debug)]
pub struct RateBounds {
    pub minimum_rate: ExchangeRate,
    pub maximum_rate: ExchangeRate,
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
            minimum_rate: ExchangeRate::from(self.minimum_micro_units)
                .saturating_mul(MICRO_UNIT_TO_RATE_SCALE),
            maximum_rate: ExchangeRate::from(self.maximum_micro_units)
                .saturating_mul(MICRO_UNIT_TO_RATE_SCALE),
            pair,
        }
    }
}

/// Absolute bounds every exchange rate must fall in, whichever source reports it.
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

/// Cross-field checks the per-field `range` attributes cannot express. Reads the micro-unit fields
/// an operator sets rather than the scaled `RateBounds`, so a rejection names the key they edited.
fn validate_all_rate_bounds_config(config: &AllRateBoundsConfig) -> Result<(), ValidationError> {
    for (pair_name, bounds) in [
        ("eth_usd", &config.eth_usd),
        ("strk_usd", &config.strk_usd),
        ("eth_strk", &config.eth_strk),
    ] {
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

/// The window a feed round's `updated_at` must fall in, relative to the block being priced.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Validate)]
#[validate(schema(function = "validate_freshness_window"))]
pub struct FreshnessWindow {
    #[validate(range(min = 1))]
    pub max_staleness_seconds: u64,
    pub max_future_updated_at_seconds: u64,
}

/// Rejects a window whose two bounds are swapped: `max_future_updated_at_seconds` covers clock
/// skew, so it must sit strictly below `max_staleness_seconds`, which covers a full feed heartbeat.
fn validate_freshness_window(freshness: &FreshnessWindow) -> Result<(), ValidationError> {
    if freshness.max_future_updated_at_seconds >= freshness.max_staleness_seconds {
        return Err(create_validation_error(
            format!(
                "max_future_updated_at_seconds ({}) is not below max_staleness_seconds ({})",
                freshness.max_future_updated_at_seconds, freshness.max_staleness_seconds
            ),
            "inverted freshness window",
            "Keep max_future_updated_at_seconds, which covers clock skew, below \
             max_staleness_seconds, which covers the feed's heartbeat.",
        ));
    }
    Ok(())
}

/// Configuration for reading Chainlink's on-chain Starknet price feeds through the batcher. Unlike
/// the per-feed `ExchangeRateOracleConfig`s, one instance of this config serves both feeds: the
/// ETH/STRK rate is derived from the same two on-chain feeds the STRK/USD rate is read from. Holds
/// only what is Chainlink-specific: the bounds a feed's answer is judged against describe the rate
/// rather than the source, so they live in `AllRateBoundsConfig`, which also bounds the derived
/// ETH/STRK pair that has no feed of its own.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Validate)]
pub struct ChainlinkOracleConfig {
    /// Quotes USD per ETH.
    pub eth_usd_feed_address: ContractAddress,
    /// Quotes USD per STRK.
    pub strk_usd_feed_address: ContractAddress,
    #[validate(nested)]
    pub freshness: FreshnessWindow,
    #[validate(range(min = 1))]
    pub sampling_interval_seconds: u64,
    #[validate(range(min = 1))]
    pub failure_retry_interval_seconds: u64,
}

impl Default for ChainlinkOracleConfig {
    fn default() -> Self {
        // Chainlink proxy addresses on Starknet mainnet. The proxies are used rather than the
        // aggregators behind them, because aggregators are rotated without notice.
        const ETH_USD_PROXY_ADDRESS: &str =
            "0x06b2ef9b416ad0f996b2a8ac0dd771b1788196f51c96f5b000df2e47ac756d26";
        const STRK_USD_PROXY_ADDRESS: &str =
            "0x076a0254cdadb59b86da3b5960bf8d73779cac88edc5ae587cab3cedf03226ec";
        // The feeds guarantee an update at least once per 24h heartbeat; the extra hour absorbs
        // the delay between the heartbeat deadline and the update landing on-chain.
        const HEARTBEAT_PLUS_MARGIN_SECONDS: u64 = (24 + 1) * 3600;
        // `updated_at` and the block timestamp it is checked against both come from a
        // sequencer's clock, so this only covers the skew between them.
        const MAX_FUTURE_UPDATED_AT_SECONDS: u64 = 300;

        Self {
            eth_usd_feed_address: parse_feed_address(ETH_USD_PROXY_ADDRESS),
            strk_usd_feed_address: parse_feed_address(STRK_USD_PROXY_ADDRESS),
            freshness: FreshnessWindow {
                max_staleness_seconds: HEARTBEAT_PLUS_MARGIN_SECONDS,
                max_future_updated_at_seconds: MAX_FUTURE_UPDATED_AT_SECONDS,
            },
            sampling_interval_seconds: 900, // 15 minutes
            // Successful reads are sampled once per sampling interval, so a failure that waited
            // for the next sample would freeze the price for that long.
            failure_retry_interval_seconds: 60,
        }
    }
}

fn parse_feed_address(hex_address: &str) -> ContractAddress {
    ContractAddress::try_from(Felt::from_hex(hex_address).expect("Invalid feed address felt"))
        .expect("Invalid feed contract address")
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
    pub eth_to_strk_oracle_source: ExchangeRateOracleSource,
    pub strk_to_usd_oracle_source: ExchangeRateOracleSource,
    // `rate_bounds_config` and `chainlink_oracle_config` apply to every feed, unlike the per-feed
    // HTTP configs, and both are validated while every feed is still on `Http`, so a bad value is
    // rejected at config load rather than at the moment an operator flips a source to `Chainlink`.
    #[validate(nested)]
    pub rate_bounds_config: AllRateBoundsConfig,
    #[validate(nested)]
    pub chainlink_oracle_config: ChainlinkOracleConfig,
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
            eth_to_strk_oracle_source: ExchangeRateOracleSource::default(),
            strk_to_usd_oracle_source: ExchangeRateOracleSource::default(),
            rate_bounds_config: AllRateBoundsConfig::default(),
            chainlink_oracle_config: ChainlinkOracleConfig::default(),
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
