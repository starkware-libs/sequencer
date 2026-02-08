use std::collections::BTreeMap;

use apollo_config::dumping::{
    prepend_sub_config_name,
    ser_optional_param,
    ser_param,
    SerializeConfig,
};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::de::Error as DeError;
use serde::ser::Error as SerError;
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

#[derive(Clone, Debug, PartialEq)]
pub struct CommitteeConfig {
    pub start_epoch: u64,
    pub committee_size: usize,
    pub stakers: Vec<ConfiguredStaker>,
}

/// Serializes `CommitteeConfig` into the format:
/// `epoch,size:addr,weight,pk,can_propose;addr,weight,pk,can_propose`
pub fn serialize_committee_config(config: &CommitteeConfig) -> Result<String, String> {
    let stakers_str = config
        .stakers
        .iter()
        .map(|s| format!("{},{},{},{}", s.address, s.weight.0, s.public_key, s.can_propose))
        .collect::<Vec<_>>()
        .join(";");
    Ok(format!("{},{}:{}", config.start_epoch, config.committee_size, stakers_str))
}

/// Parses `CommitteeConfig` from the format:
/// `epoch,size:addr,weight,pk,can_propose;addr,weight,pk,can_propose`
pub fn parse_committee_config(s: &str) -> Result<CommitteeConfig, String> {
    let trimmed = s.trim();
    let (meta_part, stakers_part) = trimmed
        .split_once(':')
        .ok_or_else(|| format!("Invalid format '{trimmed}': Missing ':' separator"))?;

    let meta_tokens: Vec<&str> = meta_part.split(',').map(|s| s.trim()).collect();
    if meta_tokens.len() != 2 {
        return Err(format!("Invalid metadata '{meta_part}': Expected 'epoch,size'"));
    }

    let start_epoch = meta_tokens[0]
        .parse::<u64>()
        .map_err(|e| format!("Invalid start_epoch '{}': {}", meta_tokens[0], e))?;
    let committee_size = meta_tokens[1]
        .parse::<usize>()
        .map_err(|e| format!("Invalid committee_size '{}': {}", meta_tokens[1], e))?;

    let mut stakers = Vec::new();
    let stakers_part_trimmed = stakers_part.trim();
    for staker_str in stakers_part_trimmed.split(';') {
        let staker_str = staker_str.trim();
        if staker_str.is_empty() {
            continue;
        }

        let parts: Vec<&str> = staker_str.split(',').map(|s| s.trim()).collect();
        if parts.len() != 4 {
            return Err(format!(
                "Invalid staker '{staker_str}': Expected 4 fields (addr,weight,pk,can_propose)"
            ));
        }

        let address =
            parts[0].parse().map_err(|e| format!("Invalid address '{}': {}", parts[0], e))?;
        let weight = parts[1]
            .parse::<u128>()
            .map_err(|e| format!("Invalid weight '{}': {}", parts[1], e))?;
        let public_key = Felt::from_hex(parts[2])
            .map_err(|e| format!("Invalid public_key '{}': {}", parts[2], e))?;
        let can_propose =
            parts[3].parse().map_err(|e| format!("Invalid can_propose '{}': {}", parts[3], e))?;

        stakers.push(ConfiguredStaker {
            address,
            weight: StakingWeight(weight),
            public_key,
            can_propose,
        });
    }

    Ok(CommitteeConfig { start_epoch, committee_size, stakers })
}

/// Deserializes `CommitteeConfig` from the format (for use with `#[serde(deserialize_with)]`).
/// `epoch,size:addr,weight,pk,can_propose;addr,weight,pk,can_propose`
pub fn deserialize_committee_config<'de, D>(de: D) -> Result<CommitteeConfig, D::Error>
where
    D: Deserializer<'de>,
{
    let raw: String = Deserialize::deserialize(de)?;
    parse_committee_config(&raw).map_err(D::Error::custom)
}

pub fn deserialize_optional_committee_config<'de, D>(
    de: D,
) -> Result<Option<CommitteeConfig>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt: Option<String> = Option::deserialize(de)?;
    match opt {
        None => Ok(None),
        Some(s) => parse_committee_config(&s).map_err(D::Error::custom).map(Some),
    }
}

impl Serialize for CommitteeConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = serialize_committee_config(self).map_err(S::Error::custom)?;
        serializer.serialize_str(&s)
    }
}

impl<'de> Deserialize<'de> for CommitteeConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserialize_committee_config(deserializer)
    }
}

/// Gets the applicable CommitteeConfig for a given epoch.
/// Returns the override config if it exists and the epoch >= override.start_epoch,
/// otherwise returns the default config.
pub fn get_config_for_epoch<'a>(
    default_config: &'a CommitteeConfig,
    override_config: &'a Option<CommitteeConfig>,
    epoch: u64,
) -> &'a CommitteeConfig {
    match override_config {
        Some(override_cfg) if epoch >= override_cfg.start_epoch => override_cfg,
        _ => {
            assert!(
                epoch >= default_config.start_epoch,
                "No committee config found for epoch {epoch}."
            );
            default_config
        }
    }
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
    // Defines the default committee configuration (size and stakers) that applies to all epochs.
    // Used by `MockStakingContract` and `StakingManager` to determine committee composition
    // and eligible proposers.
    #[serde(deserialize_with = "deserialize_committee_config")]
    pub default_committee: CommitteeConfig,

    // Optional override configuration that takes precedence over default_committee
    // for epochs >= override_committee.start_epoch.
    // This allows changing both committee size and composition at a specific epoch.
    #[serde(deserialize_with = "deserialize_optional_committee_config")]
    pub override_committee: Option<CommitteeConfig>,
}

impl Default for StakingManagerDynamicConfig {
    fn default() -> Self {
        Self {
            default_committee: CommitteeConfig {
                start_epoch: 0,
                committee_size: 100,
                stakers: Vec::new(),
            },
            override_committee: None,
        }
    }
}

impl SerializeConfig for StakingManagerDynamicConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = BTreeMap::from_iter([ser_param(
            "default_committee",
            &self.default_committee,
            "Defines the default committee configuration (size and stakers) for all epochs.",
            ParamPrivacyInput::Public,
        )]);
        config.extend(ser_optional_param(
            &self.override_committee,
            self.default_committee.clone(),
            "override_committee",
            "Optional override configuration that takes precedence over default_committee.",
            ParamPrivacyInput::Public,
        ));
        config
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct StakingManagerStaticConfig {
    pub max_cached_epochs: usize,
    pub use_only_actual_proposer_selection: bool,
}

impl Default for StakingManagerStaticConfig {
    fn default() -> Self {
        Self { max_cached_epochs: 10, use_only_actual_proposer_selection: false }
    }
}

impl SerializeConfig for StakingManagerStaticConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "max_cached_epochs",
                &self.max_cached_epochs,
                "The maximum number of epochs to cache.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "use_only_actual_proposer_selection",
                &self.use_only_actual_proposer_selection,
                "If true, get_proposer will use the same deterministic round-robin selection as \
                 get_actual_proposer.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}
