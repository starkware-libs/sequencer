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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
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

#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq)]
pub struct StakingManagerConfig {
    pub dynamic_config: StakingManagerDynamicConfig,
    pub static_config: StakingManagerStaticConfig,
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
