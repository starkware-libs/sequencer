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
use serde::{Deserialize, Serialize};
use starknet_api::core::{ChainId, ContractAddress};
use starknet_types_core::felt::Felt;
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

/// Decimals of the micro-unit sanity bounds in `ChainlinkOracleConfig`.
pub const MICRO_UNIT_DECIMALS: u32 = 6;

/// Inclusive absolute bounds on one rate, in micro units (1e-6) of the pair's quote currency, so
/// that they fit in a `u64`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct MicroUnitBounds {
    pub minimum_micro_units: u64,
    pub maximum_micro_units: u64,
}

impl SerializeConfig for MicroUnitBounds {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "minimum_micro_units",
                &self.minimum_micro_units,
                "Lowest accepted value, in micro units (1e-6) of the pair's quote currency.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "maximum_micro_units",
                &self.maximum_micro_units,
                "Highest accepted value, in micro units (1e-6) of the pair's quote currency.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

/// Configuration for reading Chainlink's on-chain Starknet price feeds through the batcher.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Validate)]
#[validate(schema(function = "validate_chainlink_oracle_config"))]
pub struct ChainlinkOracleConfig {
    pub eth_usd_feed_address: ContractAddress,
    pub strk_usd_feed_address: ContractAddress,
    #[validate(range(min = 1))]
    pub max_staleness_seconds: u64,
    pub max_future_updated_at_seconds: u64,
    /// Micro-USD per ETH.
    pub eth_usd_price_bounds: MicroUnitBounds,
    /// Micro-USD per STRK.
    pub strk_usd_price_bounds: MicroUnitBounds,
    /// Micro-STRK per ETH.
    pub eth_to_fri_rate_bounds: MicroUnitBounds,
    #[validate(range(min = 1))]
    pub lag_interval_seconds: u64,
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
        const MICRO_UNITS_PER_UNIT: u64 = 10u64.pow(MICRO_UNIT_DECIMALS);
        // The client quantizes a block timestamp into a `lag_interval_seconds` bucket and queries
        // for the bucket before it, so the queried timestamp trails the block timestamp by one to
        // two lag intervals and a round written in the block being built already reads up to 120s
        // ahead of it. The rest is headroom for skew between the feed's block timestamps and this
        // node's.
        const MAX_FUTURE_UPDATED_AT_SECONDS: u64 = 300;

        Self {
            eth_usd_feed_address: parse_feed_address(ETH_USD_PROXY_ADDRESS),
            strk_usd_feed_address: parse_feed_address(STRK_USD_PROXY_ADDRESS),
            max_staleness_seconds: HEARTBEAT_PLUS_MARGIN_SECONDS,
            max_future_updated_at_seconds: MAX_FUTURE_UPDATED_AT_SECONDS,
            // $20 .. $50,000 per ETH: ~10x above the all-time high and far below any plausible
            // market, but tight enough to reject a feed wired to a different asset.
            eth_usd_price_bounds: MicroUnitBounds {
                minimum_micro_units: 20 * MICRO_UNITS_PER_UNIT,
                maximum_micro_units: 50_000 * MICRO_UNITS_PER_UNIT,
            },
            // $0.0001 .. $10 per STRK. Wide on purpose: its job is wrong-feed detection, and the
            // fee-level damage on this leg is bounded separately by the cap on the L2 gas price.
            strk_usd_price_bounds: MicroUnitBounds {
                minimum_micro_units: MICRO_UNITS_PER_UNIT / 10_000,
                maximum_micro_units: 10 * MICRO_UNITS_PER_UNIT,
            },
            // 10,000 .. 1,000,000 STRK per ETH. The pair trades near 1.3e5, so the band spans about
            // a decade either side of spot. The floor is the load-bearing side: this rate reaches
            // L1 gas pricing through `wei_to_fri` with no ratchet and no clamp, so a poisoned feed
            // that passes both USD legs can undercharge L1 gas by at most the spot-to-floor ratio.
            eth_to_fri_rate_bounds: MicroUnitBounds {
                minimum_micro_units: 10_000 * MICRO_UNITS_PER_UNIT,
                maximum_micro_units: 1_000_000 * MICRO_UNITS_PER_UNIT,
            },
            // The bucket is also the retry period after a failed read. The feeds update on price
            // deviation rather than on a fixed schedule, so the sampling period is bounded by how
            // stale a price may be, not by a publishing cadence.
            lag_interval_seconds: 60,
        }
    }
}

/// Cross-field checks the per-field `range` attributes cannot express: a zero minimum silently
/// disables a guard, and an inverted pair rejects every reading forever.
fn validate_chainlink_oracle_config(config: &ChainlinkOracleConfig) -> Result<(), ValidationError> {
    let named_bounds = [
        ("eth_usd_price_bounds", &config.eth_usd_price_bounds),
        ("strk_usd_price_bounds", &config.strk_usd_price_bounds),
        ("eth_to_fri_rate_bounds", &config.eth_to_fri_rate_bounds),
    ];
    for (bounds_name, bounds) in named_bounds {
        if bounds.minimum_micro_units == 0 {
            return Err(create_validation_error(
                format!("{bounds_name}.minimum_micro_units is zero"),
                "zero sanity bound",
                "A zero minimum disables the lower sanity bound; set it to the lowest plausible \
                 value.",
            ));
        }
        if bounds.minimum_micro_units >= bounds.maximum_micro_units {
            return Err(create_validation_error(
                format!(
                    "{bounds_name}.minimum_micro_units ({}) is not below \
                     {bounds_name}.maximum_micro_units ({})",
                    bounds.minimum_micro_units, bounds.maximum_micro_units
                ),
                "inverted sanity bounds",
                "Ensure each minimum sanity bound is strictly below its maximum.",
            ));
        }
    }
    // See `MAX_FUTURE_UPDATED_AT_SECONDS` above for why the tolerance must exceed this.
    let minimum_future_tolerance = config.lag_interval_seconds.saturating_mul(2);
    if config.max_future_updated_at_seconds <= minimum_future_tolerance {
        return Err(create_validation_error(
            format!(
                "max_future_updated_at_seconds ({}) does not exceed twice lag_interval_seconds \
                 ({})",
                config.max_future_updated_at_seconds, config.lag_interval_seconds
            ),
            "future tolerance too small",
            "Ensure max_future_updated_at_seconds is greater than 2 * lag_interval_seconds.",
        ));
    }
    Ok(())
}

fn parse_feed_address(hex_address: &str) -> ContractAddress {
    ContractAddress::try_from(Felt::from_hex(hex_address).expect("Invalid feed address felt"))
        .expect("Invalid feed contract address")
}

impl SerializeConfig for ChainlinkOracleConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = BTreeMap::from_iter([
            ser_param(
                "eth_usd_feed_address",
                &self.eth_usd_feed_address,
                "Address of the Chainlink ETH/USD proxy feed on Starknet.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "strk_usd_feed_address",
                &self.strk_usd_feed_address,
                "Address of the Chainlink STRK/USD proxy feed on Starknet.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_staleness_seconds",
                &self.max_staleness_seconds,
                "Maximum age (seconds) of a feed's `updated_at` relative to the queried \
                 timestamp. An older reading is rejected, and for the derived ETH/STRK rate a \
                 single stale leg rejects the whole rate.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_future_updated_at_seconds",
                &self.max_future_updated_at_seconds,
                "Maximum amount (seconds) by which a feed's `updated_at` may lead the queried \
                 timestamp. Must exceed twice `lag_interval_seconds`, since the queried timestamp \
                 trails the block timestamp by that much.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "lag_interval_seconds",
                &self.lag_interval_seconds,
                "The size of the interval (seconds) that the exchange rate is quantized to, so \
                 that a block timestamp maps to a stable cache bucket. Also the period between \
                 read attempts, including after a failed read.",
                ParamPrivacyInput::Public,
            ),
        ]);
        for (bounds_name, bounds) in [
            ("eth_usd_price_bounds", &self.eth_usd_price_bounds),
            ("strk_usd_price_bounds", &self.strk_usd_price_bounds),
            ("eth_to_fri_rate_bounds", &self.eth_to_fri_rate_bounds),
        ] {
            config.extend(prepend_sub_config_name(bounds.dump(), bounds_name));
        }
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
