use std::collections::BTreeMap;
use std::fmt::Debug;
use std::time::Duration;

use apollo_config::behavior_mode::BehaviorMode;
use apollo_config::converters::{
    deserialize_milliseconds_to_duration,
    deserialize_seconds_to_duration,
    serialize_duration_as_milliseconds,
};
use apollo_config::dumping::{
    prepend_sub_config_name,
    ser_optional_param,
    ser_param,
    SerializeConfig,
};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::de::{Deserializer, Error};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use starknet_api::core::{ChainId, ContractAddress};
use url::Url;
use validator::Validate;

#[cfg(test)]
#[path = "config_test.rs"]
mod config_test;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CendeConfig {
    pub recorder_url: Url,

    // Retry policy.
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub max_retry_duration_secs: Duration,
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    pub min_retry_interval_ms: Duration,
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
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

impl SerializeConfig for CendeConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "recorder_url",
                &self.recorder_url,
                "The URL of the Pythonic cende_recorder",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_retry_duration_secs",
                &self.max_retry_duration_secs.as_secs(),
                "The maximum duration (seconds) to retry the request to the recorder",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "min_retry_interval_ms",
                &self.min_retry_interval_ms.as_millis(),
                "The minimum waiting time (milliseconds) between retries",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_retry_interval_ms",
                &self.max_retry_interval_ms.as_millis(),
                "The maximum waiting time (milliseconds) between retries",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

const GWEI_FACTOR: u128 = u128::pow(10, 9);
const ETH_FACTOR: u128 = u128::pow(10, 18);

// Default SNIP-35 target USD cost per L2 gas unit: $0.88 per 1e9 L2 gas = 880_000_000 atto-USD.
pub const DEFAULT_SNIP35_TARGET_ATTO_USD_PER_L2_GAS: u128 = 880_000_000;

// Denominator for parts-per-thousand values. Duplicates
// `apollo_consensus_orchestrator::dynamic_gas_price::PPT_DENOMINATOR`, which computes the bands
// these values configure; that crate depends on this one, so the two must stay equal.
const PPT_DENOMINATOR: u128 = 1000;

// Default per-block bound on the Eth-to-Fri rate change: 5%. Wide enough that a single feed
// update never trips it (the ETH and STRK feeds update on deviation thresholds of about 0.5% and
// 1%, so their ratio steps by at most ~1.5% between blocks), tight enough that one manipulated
// feed reading moves the rate a node publishes by at most 5%, and doubling it requires holding the
// feed for ~15 consecutive blocks. A malicious proposer reaches further than a manipulated feed
// does, since the band's center comes from the previous block: see `clamp_eth_to_fri_rate_change`
// in apollo_consensus_orchestrator.
pub const DEFAULT_MAX_ETH_TO_FRI_RATE_CHANGE_PPT: u128 = 50;

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

impl SerializeConfig for ContextConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = BTreeMap::new();
        config.extend(prepend_sub_config_name(self.dynamic_config.dump(), "dynamic_config"));
        config.extend(prepend_sub_config_name(self.static_config.dump(), "static_config"));
        config
    }
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
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    pub validate_proposal_margin_millis: Duration,
    /// The fraction (0.0 - 1.0) of the total build time allocated to waiting
    /// for the retrospective block hash to be available. The remaining time is used to build the
    /// proposal.
    pub build_proposal_time_ratio_for_retrospective_block_hash: f32,
    /// The interval between retrospective block hash retries.
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    pub retrospective_block_hash_retry_interval_millis: Duration,
    pub behavior_mode: BehaviorMode,
    /// When adding a synced block, fetch its accessed keys from the recorder so the node can build
    /// the state commitment infos locally.
    pub fetch_accessed_keys_from_centralized: bool,
    /// For each height whose state commitment infos are sent to the cende recorder, send an empty
    /// object instead of the one stored by the batcher.
    pub send_empty_state_commitment_infos_only: bool,
}

impl SerializeConfig for ContextStaticConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = BTreeMap::from_iter([
            ser_param(
                "proposal_buffer_size",
                &self.proposal_buffer_size,
                "The buffer size for streaming outbound proposals.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "chain_id",
                &self.chain_id,
                "The chain id of the Starknet chain.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "block_timestamp_window_seconds",
                &self.block_timestamp_window_seconds,
                "Maximum allowed deviation (seconds) of a proposed block's timestamp from the \
                 current time.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "l1_da_mode",
                &self.l1_da_mode,
                "The data availability mode, true: Blob, false: Calldata.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "builder_address",
                &self.builder_address,
                "The address of the contract that builds the block.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "validate_proposal_margin_millis",
                &self.validate_proposal_margin_millis.as_millis(),
                "Safety margin (in ms) to make sure that consensus determines when to timeout \
                 validating a proposal.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "build_proposal_time_ratio_for_retrospective_block_hash",
                &self.build_proposal_time_ratio_for_retrospective_block_hash,
                "The fraction (0.0 - 1.0) of the total build time allocated to waiting for the \
                 retrospective block hash to be available. The remaining time is used to build \
                 the proposal.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "retrospective_block_hash_retry_interval_millis",
                &self.retrospective_block_hash_retry_interval_millis.as_millis(),
                "The interval between retrospective block hash retries.",
                ParamPrivacyInput::Public,
            ),
        ]);
        dump.extend([ser_param(
            "behavior_mode",
            &self.behavior_mode,
            "Behavior mode: 'starknet' for production, 'echonet' for test/replay mode.",
            ParamPrivacyInput::Public,
        )]);
        dump.extend([
            ser_param(
                "fetch_accessed_keys_from_centralized",
                &self.fetch_accessed_keys_from_centralized,
                "Fetch accessed keys from the centralized recorder for synced blocks, enabling \
                 local state commitment infos construction.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "send_empty_state_commitment_infos_only",
                &self.send_empty_state_commitment_infos_only,
                "For each height whose state commitment infos are sent to the cende recorder, \
                 send an empty object instead of the one stored by the batcher.",
                ParamPrivacyInput::Public,
            ),
        ]);
        dump
    }
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
            fetch_accessed_keys_from_centralized: false,
            send_empty_state_commitment_infos_only: false,
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
    /// Maximum change, in parts per thousand, of the Eth-to-Fri rate relative to the rate implied
    /// by the previous block. A fresh oracle rate outside this band is clamped to the band's edge.
    /// Must be in `1..1000`: zero pins the rate to the previous block's forever, and a thousand or
    /// more puts the band's lower edge at zero. The bound does not apply when there is no previous
    /// block, or when `override_eth_to_fri_rate` pins the rate.
    ///
    /// Must hold the same value network-wide. A validator checks a proposal's L1 prices against
    /// its own clamped rate within `l1_gas_price_margin_percent`, so two honest nodes whose values
    /// differ by more than that margin reject each other's proposals on a large oracle move.
    pub max_eth_to_fri_rate_change_ppt: u128,
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

impl SerializeConfig for ContextDynamicConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = BTreeMap::from_iter([
            ser_param(
                "build_proposal_margin_millis",
                &self.build_proposal_margin_millis.as_millis(),
                "Safety margin (in ms) to make sure that the batcher completes building the \
                 proposal with enough time for the Fin to be checked by validators.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "min_l1_gas_price_wei",
                &self.min_l1_gas_price_wei,
                "The minimum L1 gas price in wei.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_l1_gas_price_wei",
                &self.max_l1_gas_price_wei,
                "The maximum L1 gas price in wei.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "min_l1_data_gas_price_wei",
                &self.min_l1_data_gas_price_wei,
                "The minimum L1 data gas price in wei.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_l1_data_gas_price_wei",
                &self.max_l1_data_gas_price_wei,
                "The maximum L1 data gas price in wei.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "l1_data_gas_price_multiplier_ppt",
                &self.l1_data_gas_price_multiplier_ppt,
                "Part per thousand of multiplicative factor to apply to the data gas price, to \
                 enable fine-tuning of the price charged to end users.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "l1_gas_tip_wei",
                &self.l1_gas_tip_wei,
                "This additional gas is added to the L1 gas price.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "snip35_target_atto_usd_per_l2_gas",
                &self.snip35_target_atto_usd_per_l2_gas,
                "SNIP-35 target USD cost per L2 gas unit, in atto-USD ($0.88 per 1e9 L2 gas = \
                 880_000_000 atto-USD).",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_eth_to_fri_rate_change_ppt",
                &self.max_eth_to_fri_rate_change_ppt,
                "Maximum change, in parts per thousand, of the Eth-to-Fri rate relative to the \
                 rate implied by the previous block. A fresh oracle rate outside this band is \
                 clamped to the band's edge. Must be in 1..1000, and the same network-wide.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "compare_retrospective_block_hash",
                &self.compare_retrospective_block_hash,
                "Whether to compare the retrospective block hash between the Batcher and the \
                 State Sync.",
                ParamPrivacyInput::Public,
            ),
        ]);
        dump.extend(ser_optional_param(
            &self.override_l2_gas_price_fri,
            0,
            "override_l2_gas_price_fri",
            "Replace the L2 gas price (fri) with this value.",
            ParamPrivacyInput::Public,
        ));
        dump.extend(ser_optional_param(
            &self.override_l1_gas_price_fri,
            0,
            "override_l1_gas_price_fri",
            "Replace the L1 gas price (fri) with this value.",
            ParamPrivacyInput::Public,
        ));
        dump.extend(ser_optional_param(
            &self.override_l1_data_gas_price_fri,
            0,
            "override_l1_data_gas_price_fri",
            "Replace the L1 data gas price (fri) with this value.",
            ParamPrivacyInput::Public,
        ));
        dump.extend(ser_optional_param(
            &self.override_eth_to_fri_rate,
            0,
            "override_eth_to_fri_rate",
            "Replace the Eth-to-Fri conversion rate with this value.",
            ParamPrivacyInput::Public,
        ));

        // Serialize as string format "h1:v1,h2:v2" using the same function as the Serialize impl
        let serialized = serialize_price_per_height(&self.min_l2_gas_price_per_height);
        let (key, value) = ser_param(
            "min_l2_gas_price_per_height",
            &serialized,
            "List of minimum L2 gas prices per block height in format \
             'height1:price1,height2:price2'. Each entry specifies a height and the minimum gas \
             price that applies from that height onwards.",
            ParamPrivacyInput::Public,
        );
        dump.insert(key, value);

        dump
    }
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
            max_eth_to_fri_rate_change_ppt: DEFAULT_MAX_ETH_TO_FRI_RATE_CHANGE_PPT,
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

    // Zero freezes the Eth-to-Fri rate at the previous block's, and the denominator or more leaves
    // the band one-sided, with its lower edge at zero.
    if config.max_eth_to_fri_rate_change_ppt == 0
        || config.max_eth_to_fri_rate_change_ppt >= PPT_DENOMINATOR
    {
        return Err(validator::ValidationError::new(
            "max_eth_to_fri_rate_change_ppt must be greater than zero and less than 1000",
        ));
    }

    Ok(())
}
