use std::collections::BTreeMap;

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct StakingManagerConfig {
    pub max_cached_epochs: usize,

    // The desired number of committee members to select from the available stakers.
    // If there are fewer stakers than `committee_size`, a smaller committee will be selected.
    // TODO(Dafna): Make this a dynamic config.
    pub committee_size: usize,

    // Defines how many heights in advance the proposer can be predicted.
    // While the exact identity may depend on staker prediction constraints,
    // the proposer selection logic becomes deterministic at this offset.
    // TODO(Dafna): get the block hash of the first block in the previous epoch and remove this
    // field.
    pub proposer_prediction_window_in_heights: u64,
}

impl SerializeConfig for StakingManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "max_cached_epochs",
                &self.max_cached_epochs,
                "The maximum number of epochs to cache",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "committee_size",
                &self.committee_size,
                "The desired number of committee members to select from the available stakers",
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
