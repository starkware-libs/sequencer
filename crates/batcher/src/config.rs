use std::collections::BTreeMap;

use papyrus_config::dumping::{append_sub_config_name, SerializeConfig};
use papyrus_config::{ParamPath, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::proposals_manager::ProposalsManagerConfig;

/// The batcher related configuration.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
pub struct BatcherConfig {
    pub proposals_manager: ProposalsManagerConfig,
    pub papyrus_storage: papyrus_storage::StorageConfig,
}

impl SerializeConfig for BatcherConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter(
            [
                append_sub_config_name(self.proposals_manager.dump(), "proposals_manager"),
                append_sub_config_name(self.papyrus_storage.dump(), "papyrus_storage"),
            ]
            .into_iter()
            .flatten(),
        )
    }
}
