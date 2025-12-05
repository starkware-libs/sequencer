use std::collections::BTreeMap;

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Validate)]
pub struct CommitterConfig {
    // TODO(Yoav): Replace with real committer configuration parameters.
    pub enable_committer: bool,
}

impl SerializeConfig for CommitterConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([ser_param(
            "enable_committer",
            &self.enable_committer,
            "Placeholder configuration.",
            ParamPrivacyInput::Public,
        )])
    }
}
