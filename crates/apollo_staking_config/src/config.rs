use std::collections::BTreeMap;

use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
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
pub struct CommitteeConfig {
    pub start_epoch: u64,
    pub committee_size: usize,
    pub stakers: Vec<ConfiguredStaker>,
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

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
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
    pub default_committee: CommitteeConfig,

    // Optional override configuration that takes precedence over default_committee
    // for epochs >= override_committee.start_epoch.
    // This allows changing both committee size and composition at a specific epoch.
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
        BTreeMap::from_iter([
            ser_param(
                "default_committee",
                &self.default_committee,
                "Defines the default committee configuration (size and stakers) for all epochs.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "override_committee",
                &self.override_committee,
                "Optional override configuration that takes precedence over default_committee.",
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
