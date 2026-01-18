use std::collections::BTreeMap;

use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::core::ContractAddress;
use validator::Validate;

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

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Default, Validate)]
pub struct StakingManagerDynamicConfig {
    // The desired number of committee members to select from the available stakers.
    // If there are fewer stakers than `committee_size`, a smaller committee will be selected.
    // TODO(Dafna): Add an epoch, from which this config should be applied.
    pub committee_size: usize,
}

impl SerializeConfig for StakingManagerDynamicConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([ser_param(
            "committee_size",
            &self.committee_size,
            "The desired number of committee members to select from the available stakers",
            ParamPrivacyInput::Public,
        )])
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct StakingManagerStaticConfig {
    pub staking_contract_address: ContractAddress,
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
                "staking_contract_address",
                &self.staking_contract_address,
                "The address of the staking contract",
                ParamPrivacyInput::Public,
            ),
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
