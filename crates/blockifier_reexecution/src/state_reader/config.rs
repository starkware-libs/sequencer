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

#[cfg(test)]
impl RpcStateReaderConfig {
    pub fn create_for_testing() -> Self {
        Self::from_url("http://localhost:8080".to_string())
    }
}
