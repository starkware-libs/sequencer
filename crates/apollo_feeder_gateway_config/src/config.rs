use std::collections::BTreeMap;

use apollo_config::dumping::SerializeConfig;
use apollo_config::{ParamPath, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

/// Configuration for the feeder gateway component.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
pub struct FeederGatewayConfig {}

impl SerializeConfig for FeederGatewayConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::new()
    }
}
