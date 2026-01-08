use std::collections::BTreeMap;

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

const JSON_RPC_VERSION: &str = "2.0";

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct RpcStateReaderConfig {
    pub url: String,
    pub json_rpc_version: String,
}

impl RpcStateReaderConfig {
    pub fn from_url(url: String) -> Self {
        Self { url, ..Default::default() }
    }
}

impl Default for RpcStateReaderConfig {
    fn default() -> Self {
        Self { url: Default::default(), json_rpc_version: JSON_RPC_VERSION.to_string() }
    }
}

#[cfg(any(feature = "testing", test))]
impl RpcStateReaderConfig {
    pub fn create_for_testing() -> Self {
        Self::from_url("http://localhost:8080".to_string())
    }
}

impl SerializeConfig for RpcStateReaderConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param("url", &self.url, "The url of the rpc server.", ParamPrivacyInput::Public),
            ser_param(
                "json_rpc_version",
                &self.json_rpc_version,
                "The json rpc version.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}
