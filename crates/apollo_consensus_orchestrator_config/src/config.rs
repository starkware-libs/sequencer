use std::collections::BTreeMap;
use std::fmt::Debug;
use std::time::Duration;

use apollo_config::converters::{
    deserialize_comma_separated_str,
    deserialize_milliseconds_to_duration,
    deserialize_seconds_to_duration,
    serialize_optional_comma_separated,
};
use apollo_config::dumping::{
    prepend_sub_config_name,
    ser_optional_param,
    ser_param,
    SerializeConfig,
};
use apollo_config::secrets::Sensitive;
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::de::{Deserializer, Error};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use starknet_api::core::{ChainId, ContractAddress};
use url::Url;
use validator::Validate;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CendeConfig {
    pub recorder_url: Sensitive<Url>,

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
                .expect("recorder_url must be a valid Recorder URL")
                .into(),
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
                // TODO(victork): make sure we're allowed to expose the recorder URL here
                self.recorder_url.peek_secret(),
                "The URL of the Pythonic cende_recorder",
                ParamPrivacyInput::Private,
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

/// Dynamic configuration for the consensus orchestrator context.
/// Can be updated at runtime via the config manager.
#[derive(Clone, Debug, PartialEq, Default, Validate)]
#[validate(schema(function = "validate_price_per_height"))]
pub struct ContextDynamicConfig {
    // List of minimum L2 gas prices per block height.
    // Format in JSON: "height1:price1,height2:price2,height3:price3"
    pub min_l2_gas_price_per_height: Vec<PricePerHeight>,
}

/// Deserializes `ContextDynamicConfig` from the string format.
/// "height1:price1,height2:price2,height3:price3"
pub fn deserialize_context_dynamic_config<'de, D>(de: D) -> Result<ContextDynamicConfig, D::Error>
where
    D: Deserializer<'de>,
{
    let raw: String = Deserialize::deserialize(de)?;
    let min_l2_gas_price_per_height = parse_price_per_height(&raw).map_err(Error::custom)?;
    Ok(ContextDynamicConfig { min_l2_gas_price_per_height })
}

impl Serialize for ContextDynamicConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = serialize_price_per_height(&self.min_l2_gas_price_per_height);
        serializer.serialize_str(&s)
    }
}

impl<'de> Deserialize<'de> for ContextDynamicConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserialize_context_dynamic_config(deserializer)
    }
}

fn validate_price_per_height(
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

impl SerializeConfig for ContextDynamicConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = BTreeMap::new();

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

/// Configuration for the Context struct.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct ContextConfig {
    /// Buffer size for streaming outbound proposals.
    pub proposal_buffer_size: usize,
    /// The number of validators.
    pub num_validators: u64,
    /// Optional explicit set of validator IDs (contract addresses) to use.
    /// If provided, this overrides `num_validators`.
    #[serde(default, deserialize_with = "deserialize_comma_separated_str")]
    pub validator_ids: Option<Vec<ContractAddress>>,
    /// The chain id of the Starknet chain.
    pub chain_id: ChainId,
    /// Maximum allowed deviation (seconds) of a proposed block's timestamp from the current time.
    pub block_timestamp_window_seconds: u64,
    /// The data availability mode, true: Blob, false: Calldata.
    pub l1_da_mode: bool,
    /// The address of the contract that builds the block.
    pub builder_address: ContractAddress,
    /// Safety margin in milliseconds to make sure that the batcher completes building the proposal
    /// with enough time for the Fin to be checked by validators.
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    pub build_proposal_margin_millis: Duration,
    // When validating a proposal the Context is responsible for timeout handling. The Batcher
    // though has a timeout as a defensive measure to make sure the proposal doesn't live
    // forever if the Context crashes or has a bug.
    /// Safety margin in milliseconds to allow the batcher to successfully validate a proposal.
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    pub validate_proposal_margin_millis: Duration,
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
    /// If given, will override the L2 gas price.
    pub override_l2_gas_price_fri: Option<u128>,
    /// If given, will override the L1 gas price.
    pub override_l1_gas_price_wei: Option<u128>,
    /// If given, will override the L1 data gas price.
    pub override_l1_data_gas_price_wei: Option<u128>,
    /// If given, will override the conversion rate.
    pub override_eth_to_fri_rate: Option<u128>,
    /// The fraction (0.0 - 1.0) of the total build time allocated to waiting
    /// for the retrospective block hash to be available. The remaining time is used to build the
    /// proposal.
    pub build_proposal_time_ratio_for_retrospective_block_hash: f32,
    /// The interval between retrospective block hash retries.
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    pub retrospective_block_hash_retry_interval_millis: Duration,
    /// Dynamic configuration that can be updated at runtime.
    #[validate(nested)]
    pub dynamic_config: ContextDynamicConfig,
}

impl SerializeConfig for ContextConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = BTreeMap::from_iter([
            ser_param(
                "proposal_buffer_size",
                &self.proposal_buffer_size,
                "The buffer size for streaming outbound proposals.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "num_validators",
                &self.num_validators,
                "The number of validators.",
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
                "build_proposal_margin_millis",
                &self.build_proposal_margin_millis.as_millis(),
                "Safety margin (in ms) to make sure that the batcher completes building the \
                 proposal with enough time for the Fin to be checked by validators.",
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
        dump.extend(ser_optional_param(
            &self.override_l2_gas_price_fri,
            0,
            "override_l2_gas_price_fri",
            "Replace the L2 gas price (fri) with this value.",
            ParamPrivacyInput::Public,
        ));
        dump.extend(ser_optional_param(
            &self.override_l1_gas_price_wei,
            0,
            "override_l1_gas_price_wei",
            "Replace the L1 gas price (wei) with this value.",
            ParamPrivacyInput::Public,
        ));
        dump.extend(ser_optional_param(
            &self.override_l1_data_gas_price_wei,
            0,
            "override_l1_data_gas_price_wei",
            "Replace the L1 data gas price (wei) with this value.",
            ParamPrivacyInput::Public,
        ));
        dump.extend(ser_optional_param(
            &self.override_eth_to_fri_rate,
            0,
            "override_eth_to_fri_rate",
            "Replace the Eth-to-Fri conversion rate with this value.",
            ParamPrivacyInput::Public,
        ));
        dump.extend(ser_optional_param(
            &serialize_optional_comma_separated(&self.validator_ids),
            "".to_string(),
            "validator_ids",
            "Optional explicit set of validator IDs (comma separated).",
            ParamPrivacyInput::Public,
        ));
        dump.extend(prepend_sub_config_name(self.dynamic_config.dump(), "dynamic_config"));

        dump
    }
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            proposal_buffer_size: 100,
            num_validators: 1,
            validator_ids: None,
            chain_id: ChainId::Mainnet,
            block_timestamp_window_seconds: 1,
            l1_da_mode: true,
            builder_address: ContractAddress::default(),
            build_proposal_margin_millis: Duration::from_millis(1000),
            validate_proposal_margin_millis: Duration::from_millis(10_000),
            min_l1_gas_price_wei: GWEI_FACTOR,
            max_l1_gas_price_wei: 200 * GWEI_FACTOR,
            min_l1_data_gas_price_wei: 1,
            max_l1_data_gas_price_wei: ETH_FACTOR,
            l1_data_gas_price_multiplier_ppt: 135,
            l1_gas_tip_wei: GWEI_FACTOR,
            override_l2_gas_price_fri: None,
            override_l1_gas_price_wei: None,
            override_l1_data_gas_price_wei: None,
            override_eth_to_fri_rate: None,
            build_proposal_time_ratio_for_retrospective_block_hash: 0.7,
            retrospective_block_hash_retry_interval_millis: Duration::from_millis(500),
            dynamic_config: ContextDynamicConfig::default(),
        }
    }
}
