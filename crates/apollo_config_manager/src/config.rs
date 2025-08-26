use std::collections::BTreeMap;

use apollo_config::dumping::SerializeConfig;
use apollo_config::{ParamPath, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Serialize, Clone, Default, PartialEq, Validate)]
pub struct ConfigManagerConfig {}

impl SerializeConfig for ConfigManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::new()
    }
}
