use std::collections::BTreeMap;

use apollo_config::dumping::{prepend_sub_config_name, SerializeConfig};
use apollo_config::{ParamPath, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_committer::block_committer::input::ReaderConfig;
use validator::Validate;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Validate)]
pub struct CommitterConfig {
    pub reader_config: ReaderConfig,
}

impl SerializeConfig for CommitterConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = BTreeMap::new();
        config.extend(prepend_sub_config_name(self.reader_config.dump(), "reader_config"));
        config
    }
}
