use std::collections::BTreeMap;

use papyrus_config::dumping::{append_sub_config_name, ser_optional_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_consensus::config::ConsensusConfig;
use starknet_consensus::types::ContextConfig;
use starknet_consensus_orchestrator::cende::CendeConfig;
use validator::Validate;

/// The consensus manager related configuration.
/// TODO(Matan): Remove ConsensusManagerConfig if it's only field remains ConsensusConfig.
#[derive(Clone, Default, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct ConsensusManagerConfig {
    pub consensus_config: ConsensusConfig,
    pub context_config: ContextConfig,
    pub cende_config: CendeConfig,
    pub revert_up_to_and_including: Option<BlockNumber>,
}

impl SerializeConfig for ConsensusManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = BTreeMap::new();

        config.extend(append_sub_config_name(self.consensus_config.dump(), "consensus_config"));
        config.extend(append_sub_config_name(self.context_config.dump(), "context_config"));
        config.extend(append_sub_config_name(self.cende_config.dump(), "cende_config"));

        // TODO(dvir): when there will be an option to have an optional config value, which is also
        // a pointer, use it instead.
        config.extend(ser_optional_param(
            &self.revert_up_to_and_including,
            // Use u64::MAX as a placeholder to prevent setting this value to
            // a low block number by mistake, which will cause significant revert operations.
            BlockNumber(u64::MAX),
            "revert_up_to_and_including",
            "The batcher will revert blocks up to this block number (including). Use this \
             configurations carefully to prevent significant revert operations and data loss.",
            ParamPrivacyInput::Private,
        ));

        config
    }
}
