use std::fmt::Debug;
use std::time::Duration;

use apollo_config::behavior_mode::BehaviorMode;
use apollo_config::converters::{
    deserialize_milliseconds_to_duration,
    deserialize_seconds_to_duration,
    serialize_duration_as_milliseconds,
    serialize_duration_as_seconds,
};
use serde::de::{Deserializer, Error};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use starknet_api::core::{ChainId, ContractAddress};
use url::Url;
use validator::Validate;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CendeConfig {
    pub recorder_url: Url,

    // Retry policy.
    #[serde(
        deserialize_with = "deserialize_seconds_to_duration",
        serialize_with = "serialize_duration_as_seconds"
    )]
    pub max_retry_duration_secs: Duration,
    #[serde(
        deserialize_with = "deserialize_milliseconds_to_duration",
        serialize_with = "serialize_duration_as_milliseconds"
    )]
    pub min_retry_interval_ms: Duration,
    #[serde(
        deserialize_with = "deserialize_milliseconds_to_duration",
        serialize_with = "serialize_duration_as_milliseconds"
    )]
    pub max_retry_interval_ms: Duration,
}

impl Default for CendeConfig {
    fn default() -> Self {
        CendeConfig {
            recorder_url: "https://recorder_url"
                .parse::<Url>()
                .expect("recorder_url must be a valid Recorder URL"),
            max_retry_duration_secs: Duration::from_secs(3),
            min_retry_interval_ms: Duration::from_millis(50),
            max_retry_interval_ms: Duration::from_secs(1),
        }
    }
}

const GWEI_FACTOR: u128 = u128::pow(10, 9);
const ETH_FACTOR: u128 = u128::pow(10, 18);

// Default SNIP-35 target USD cost per L2 gas unit: $0.88 per 1e9 L2 gas = 880_000_000 atto-USD.
pub const DEFAULT_SNIP35_TARGET_ATTO_USD_PER_L2_GAS: u128 = 880_000_000;

// This matches the min_gas_price in orchestrator_versioned_constants_0_14_1.json (0x1dcd65000).
const MIN_ALLOWED_GAS_PRICE: u128 = 8_000_000_000;

/// Represents a minimum gas price that applies starting from a specific block height.
#[derive(Debug, Clone, PartialEq)]
pub struct PricePerHeight {
    /// The block height at which this price becomes active.
    pub height: u64,
    /// The minimum gas price in fri.
    pub price: u128,
}

/// Serializes `Vec<PricePerHeight>` into the format: "height1:price1,height2:price2,height3:price3"
pub fn serialize_price_per_height(entries: &[PricePerHeight]) -> String {
    entries.iter().map(|e| format!("{}:{}", e.height, e.price)).collect::<Vec<_>>().join(",")
}

/// Parses `Vec<PricePerHeight>` from the format: "height1:price1,height2:price2,height3:price3"
pub fn parse_price_per_height(s: &str) -> Result<Vec<PricePerHeight>, String> {
    let trimmed = s.trim();

    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    trimmed
        .split(',')
        .map(|entry| {
            let entry = entry.trim();
            let parts: Vec<&str> = entry.split(':').map(|p| p.trim()).collect();
            if parts.len() != 2 {
                return Err(format!(
                    "Invalid price_per_height entry format: '{}'. Expected 'height:price'",
                    entry
                ));
            }
            let height = parts[0]
                .parse::<u64>()
                .map_err(|e| format!("Invalid height '{}': {}", parts[0], e))?;
            let price = parts[1]
                .parse::<u128>()
                .map_err(|e| format!("Invalid price '{}': {}", parts[1], e))?;
            Ok(PricePerHeight { height, price })
        })
        .collect()
}

/// Configuration for the Context struct.
#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct ContextConfig {
    #[validate(nested)]
    pub dynamic_config: ContextDynamicConfig,
    #[validate(nested)]
    pub static_config: ContextStaticConfig,
}

/// Static configuration for the Context struct.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct ContextStaticConfig {
    /// Buffer size for streaming outbound proposals.
    pub proposal_buffer_size: usize,
    /// The chain id of the Starknet chain.
    pub chain_id: ChainId,
    /// Maximum allowed deviation (seconds) of a proposed block's timestamp from the current time.
    pub block_timestamp_window_seconds: u64,
    /// The data availability mode, true: Blob, false: Calldata.
    pub l1_da_mode: bool,
    /// The address of the contract that builds the block.
    pub builder_address: ContractAddress,
    // When validating a proposal the Context is responsible for timeout handling. The Batcher
    // though has a timeout as a defensive measure to make sure the proposal doesn't live
    // forever if the Context crashes or has a bug.
    /// Safety margin in milliseconds to allow the batcher to successfully validate a proposal.
    #[serde(
        deserialize_with = "deserialize_milliseconds_to_duration",
        serialize_with = "serialize_duration_as_milliseconds"
    )]
    pub validate_proposal_margin_millis: Duration,
    /// The fraction (0.0 - 1.0) of the total build time allocated to waiting
    /// for the retrospective block hash to be available. The remaining time is used to build the
    /// proposal.
    pub build_proposal_time_ratio_for_retrospective_block_hash: f32,
    /// The interval between retrospective block hash retries.
    #[serde(
        deserialize_with = "deserialize_milliseconds_to_duration",
        serialize_with = "serialize_duration_as_milliseconds"
    )]
    pub retrospective_block_hash_retry_interval_millis: Duration,
    pub behavior_mode: BehaviorMode,
}

impl Default for ContextStaticConfig {
    fn default() -> Self {
        Self {
            proposal_buffer_size: 100,
            chain_id: ChainId::Mainnet,
            block_timestamp_window_seconds: 1,
            l1_da_mode: true,
            builder_address: ContractAddress::default(),
            validate_proposal_margin_millis: Duration::from_millis(10_000),
            build_proposal_time_ratio_for_retrospective_block_hash: 0.7,
            retrospective_block_hash_retry_interval_millis: Duration::from_millis(500),
            behavior_mode: BehaviorMode::default(),
        }
    }
}

/// Dynamic configuration for the Context struct.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Validate)]
#[validate(schema(function = "validate_dynamic_config"))]
pub struct ContextDynamicConfig {
    /// Safety margin in milliseconds to make sure that the batcher completes building the proposal
    /// with enough time for the Fin to be checked by validators.
    #[serde(
        deserialize_with = "deserialize_milliseconds_to_duration",
        serialize_with = "serialize_duration_as_milliseconds"
    )]
    pub build_proposal_margin_millis: Duration,
    /// The minimum L1 gas price in wei.
    pub min_l1_gas_price_wei: u128,
    /// The maximum L1 gas price in wei.
    pub max_l1_gas_price_wei: u128,
    /// The minimum L1 data gas price in wei.
    pub min_l1_data_gas_price_wei: u128,
    /// The maximum L1 data gas price in wei.
    pub max_l1_data_gas_price_wei: u128,
    /// Part per thousand of multiplicative factor to apply to the data gas price, to enable
    /// fine-tuning of the price charged to end users. Commonly used to apply a discount due to
    /// the blob's data being compressed. Can be used to raise the prices in case of blob
    /// under-utilization.
    pub l1_data_gas_price_multiplier_ppt: u128,
    /// This additional gas is added to the L1 gas price.
    pub l1_gas_tip_wei: u128,
    /// SNIP-35 target USD cost per L2 gas unit, in atto-USD ($0.88 per 1e9 L2 gas = 880_000_000
    /// atto-USD).
    pub snip35_target_atto_usd_per_l2_gas: u128,
    /// If given, will override the L2 gas price.
    pub override_l2_gas_price_fri: Option<u128>,
    /// If given, will override the L1 gas price in FRI.
    pub override_l1_gas_price_fri: Option<u128>,
    /// If given, will override the L1 data gas price in FRI.
    pub override_l1_data_gas_price_fri: Option<u128>,
    // TODO(guyn): remove this after we completely remove wei prices from block info.
    /// If given, will override the conversion rate.
    pub override_eth_to_fri_rate: Option<u128>,
    // List of minimum L2 gas prices per block height.
    // Format: "height1:price1,height2:price2,height3:price3"
    #[serde(
        deserialize_with = "deserialize_price_per_height_from_string",
        serialize_with = "serialize_price_per_height_as_string"
    )]
    pub min_l2_gas_price_per_height: Vec<PricePerHeight>,
    pub compare_retrospective_block_hash: bool,
}

impl Default for ContextDynamicConfig {
    fn default() -> Self {
        Self {
            build_proposal_margin_millis: Duration::from_millis(1000),
            min_l1_gas_price_wei: GWEI_FACTOR,
            max_l1_gas_price_wei: 200 * GWEI_FACTOR,
            min_l1_data_gas_price_wei: 1,
            max_l1_data_gas_price_wei: ETH_FACTOR,
            l1_data_gas_price_multiplier_ppt: 135,
            l1_gas_tip_wei: GWEI_FACTOR,
            snip35_target_atto_usd_per_l2_gas: DEFAULT_SNIP35_TARGET_ATTO_USD_PER_L2_GAS,
            override_l2_gas_price_fri: None,
            override_l1_gas_price_fri: None,
            override_l1_data_gas_price_fri: None,
            override_eth_to_fri_rate: None,
            min_l2_gas_price_per_height: vec![],
            compare_retrospective_block_hash: true,
        }
    }
}

/// Deserializes `Vec<PricePerHeight>` from string format "height1:price1,height2:price2,...".
pub fn deserialize_price_per_height_from_string<'de, D>(
    de: D,
) -> Result<Vec<PricePerHeight>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw: String = Deserialize::deserialize(de)?;
    parse_price_per_height(&raw).map_err(Error::custom)
}

/// Serializes `Vec<PricePerHeight>` as string format "height1:price1,height2:price2,...".
pub fn serialize_price_per_height_as_string<S>(
    entries: &[PricePerHeight],
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let s = serialize_price_per_height(entries);
    serializer.serialize_str(&s)
}

fn validate_dynamic_config(
    config: &ContextDynamicConfig,
) -> Result<(), validator::ValidationError> {
    // Check that heights are in strictly ascending order using windows
    if !config.min_l2_gas_price_per_height.windows(2).all(|w| w[0].height < w[1].height) {
        return Err(validator::ValidationError::new(
            "min_l2_gas_price_per_height heights must be in strictly ascending order",
        ));
    }

    // Check that all prices are above the minimum
    for entry in &config.min_l2_gas_price_per_height {
        if entry.price < MIN_ALLOWED_GAS_PRICE {
            return Err(validator::ValidationError::new(
                "all prices in min_l2_gas_price_per_height must be at least 8 gwei (8000000000 \
                 fri)",
            ));
        }
    }

    Ok(())
}
