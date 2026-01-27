use std::collections::BTreeMap;

use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use starknet_api::core::ContractAddress;
use starknet_api::staking::StakingWeight;
use starknet_types_core::felt::Felt;
use validator::Validate;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ConfiguredStaker {
    pub address: ContractAddress,
    pub weight: StakingWeight,
    pub public_key: Felt,
    pub can_propose: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct StakersConfig {
    pub start_epoch: u64,
    pub stakers: Vec<ConfiguredStaker>,
}

/// Finds the applicable StakersConfig for a given epoch.
/// Returns the config with the highest start_epoch that is <= the given epoch.
/// Returns None if no config applies to the given epoch.
pub fn find_config_for_epoch(configs: &[StakersConfig], epoch: u64) -> Option<&StakersConfig> {
    configs.iter().filter(|entry| epoch >= entry.start_epoch).max_by_key(|entry| entry.start_epoch)
}

/// Serializes `Vec<StakersConfig>` into the format:
/// `[(epoch,[(addr,weight,pubkey,can_propose),...]),...]`
pub fn serialize_stakers_config(configs: &[StakersConfig]) -> String {
    format!(
        "[{}]",
        configs
            .iter()
            .map(|config| {
                let stakers_str = config
                    .stakers
                    .iter()
                    .map(|s| {
                        format!("({},{},{},{})", s.address, s.weight.0, s.public_key, s.can_propose)
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                format!("({},[{}])", config.start_epoch, stakers_str)
            })
            .collect::<Vec<_>>()
            .join(",")
    )
}

/// Serializer for serde's `serialize_with` attribute.
pub fn serialize_stakers_config_serde<S>(
    configs: &[StakersConfig],
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&serialize_stakers_config(configs))
}

/// Deserializes `Vec<StakersConfig>` from the format:
/// `[(epoch,[(addr,weight,pubkey,can_propose),...]),...]`
pub fn deserialize_stakers_config<'de, D>(de: D) -> Result<Vec<StakersConfig>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(de)?.trim().to_string();
    if s.is_empty() || s == "[]" {
        return Ok(Vec::new());
    }
    parse_stakers_config(&s).map_err(D::Error::custom)
}

/// Helper to unwrap brackets or parentheses with validation.
fn unwrap_brackets<'a>(
    s: &'a str,
    open: char,
    close: char,
    expected: &str,
) -> Result<&'a str, String> {
    let trimmed = s.trim();
    if !trimmed.starts_with(open) || !trimmed.ends_with(close) {
        return Err(format!("Expected {expected}, got: {trimmed}"));
    }
    Ok(&trimmed[1..trimmed.len() - 1])
}

/// Parses a single staker tuple: `(addr,weight,pubkey,can_propose)`
fn parse_staker_tuple(s: &str) -> Result<ConfiguredStaker, String> {
    let content = unwrap_brackets(s, '(', ')', "ConfiguredStaker tuple")?;
    let parts: Vec<&str> = content.split(',').map(|p| p.trim()).collect();

    if parts.len() != 4 {
        return Err(format!("Expected 4 fields, found {}: {s}", parts.len()));
    }

    Ok(ConfiguredStaker {
        address: parts[0].parse().map_err(|e| format!("address '{0}': {e}", parts[0]))?,
        weight: StakingWeight(
            parts[1].parse::<u128>().map_err(|e| format!("weight '{0}': {e}", parts[1]))?,
        ),
        public_key: parts[2].parse().map_err(|e| format!("public_key '{0}': {e}", parts[2]))?,
        can_propose: parts[3]
            .parse::<bool>()
            .map_err(|e| format!("can_propose '{0}': {e}", parts[3]))?,
    })
}

/// Parses a single config tuple: `(epoch, [stakers])`
fn parse_config_tuple(s: &str) -> Result<StakersConfig, String> {
    let content = unwrap_brackets(s, '(', ')', "StakersConfig tuple")?;
    let (epoch_str, list_str) =
        content.split_once(',').ok_or_else(|| format!("Missing comma: {s}"))?;

    let start_epoch =
        epoch_str.trim().parse::<u64>().map_err(|e| format!("epoch '{epoch_str}': {e}"))?;
    let list_content = unwrap_brackets(list_str.trim(), '[', ']', "staker list")?;

    let stakers = split_respecting_brackets(list_content, ',')
        .iter()
        .filter_map(|s| {
            let s = s.trim();
            if s.is_empty() { None } else { Some(parse_staker_tuple(s)) }
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(StakersConfig { start_epoch, stakers })
}

/// Parses the full stakers config format.
fn parse_stakers_config(s: &str) -> Result<Vec<StakersConfig>, String> {
    let content = unwrap_brackets(s, '[', ']', "outer list")?;
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }

    split_respecting_brackets(content, ',')
        .iter()
        .filter_map(|s| {
            let s = s.trim();
            if s.is_empty() { None } else { Some(parse_config_tuple(s)) }
        })
        .collect()
}

/// Helper function to split a string by a delimiter, but ignore delimiters
/// inside nested parentheses () or brackets [].
fn split_respecting_brackets(input: &str, delimiter: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut last_index = 0;
    let mut level = 0;

    for (i, c) in input.char_indices() {
        match c {
            '(' | '[' => level += 1,
            ')' | ']' => level -= 1,
            _ if level == 0 && c == delimiter => {
                parts.push(&input[last_index..i]);
                last_index = i + 1;
            }
            _ => {}
        }
    }
    if last_index <= input.len() {
        parts.push(&input[last_index..]);
    }
    parts
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq)]
pub struct StakingManagerConfig {
    pub dynamic_config: StakingManagerDynamicConfig,
    pub static_config: StakingManagerStaticConfig,
}

impl SerializeConfig for StakingManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = BTreeMap::new();
        config.extend(prepend_sub_config_name(self.dynamic_config.dump(), "dynamic_config"));
        config.extend(prepend_sub_config_name(self.static_config.dump(), "static_config"));
        config
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct StakingManagerDynamicConfig {
    // The desired number of committee members to select from the available stakers.
    // If there are fewer stakers than `committee_size`, a smaller committee will be selected.
    // TODO(Dafna): Add an epoch, from which this config should be applied.
    pub committee_size: usize,

    // Defines the set of stakers per epoch.
    // Used by `MockStakingContract` and `StakingManager` to determine eligible proposers.
    // Each entry applies from its start_epoch until overridden by a later entry.
    #[serde(deserialize_with = "deserialize_stakers_config")]
    pub stakers_config: Vec<StakersConfig>,
}

impl Default for StakingManagerDynamicConfig {
    fn default() -> Self {
        Self { committee_size: 100, stakers_config: Vec::new() }
    }
}

impl SerializeConfig for StakingManagerDynamicConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "committee_size",
                &self.committee_size,
                "The desired number of committee members to select from the available stakers",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "stakers_config",
                &serialize_stakers_config(&self.stakers_config),
                "Defines the set of stakers per epoch.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct StakingManagerStaticConfig {
    pub max_cached_epochs: usize,

    // Defines how many heights in advance the proposer can be predicted.
    // While the exact identity may depend on staker prediction constraints,
    // the proposer selection logic becomes deterministic at this offset.
    // TODO(Dafna): get the block hash of the first block in the previous epoch and remove this
    // field.
    pub proposer_prediction_window_in_heights: u64,
}

impl Default for StakingManagerStaticConfig {
    fn default() -> Self {
        Self { max_cached_epochs: 10, proposer_prediction_window_in_heights: 10 }
    }
}

impl SerializeConfig for StakingManagerStaticConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "max_cached_epochs",
                &self.max_cached_epochs,
                "The maximum number of epochs to cache",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "proposer_prediction_window_in_heights",
                &self.proposer_prediction_window_in_heights,
                "Defines how many heights in advance the proposer can be predicted",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}
