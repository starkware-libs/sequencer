use std::collections::BTreeMap;

use papyrus_config::dumping::{append_sub_config_name, SerializeConfig};
use papyrus_config::{ParamPath, SerializedParam};
use papyrus_consensus::config::ConsensusConfig;
use serde::{Deserialize, Serialize};
use validator::Validate;

/// The consensus manager related configuration.
/// TODO(Matan): Remove ConsensusManagerConfig if it's only field remains ConsensusConfig.
#[derive(Clone, Default, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct ConsensusManagerConfig {
    pub consensus_config: ConsensusConfig,
}

impl SerializeConfig for ConsensusManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let sub_configs =
            vec![append_sub_config_name(self.consensus_config.dump(), "consensus_config")];

        sub_configs.into_iter().flatten().collect()
    }
}
