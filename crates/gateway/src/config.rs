use std::collections::BTreeMap;
use std::net::IpAddr;

use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

/// The gateway network connection related configuration.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct GatewayNetworkConfig {
    pub ip: IpAddr,
    pub port: u16,
}

impl SerializeConfig for GatewayNetworkConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "ip",
                &self.ip.to_string(),
                "The gateway server ip.",
                ParamPrivacyInput::Public,
            ),
            ser_param("port", &self.port, "The gateway server port.", ParamPrivacyInput::Public),
        ])
    }
}

impl Default for GatewayNetworkConfig {
    fn default() -> Self {
        Self { ip: "0.0.0.0".parse().unwrap(), port: 8080 }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
pub struct StatelessTransactionValidatorConfig {
    // If true, validates that the resource bounds are not zero.
    pub validate_non_zero_l1_gas_fee: bool,
    pub validate_non_zero_l2_gas_fee: bool,

    pub max_calldata_length: usize,
    pub max_signature_length: usize,
}

impl SerializeConfig for StatelessTransactionValidatorConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "validate_non_zero_l1_gas_fee",
                &self.validate_non_zero_l1_gas_fee,
                "If true, validates that a transaction has non-zero L1 resource bounds.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "validate_non_zero_l2_gas_fee",
                &self.validate_non_zero_l2_gas_fee,
                "If true, validates that a transaction has non-zero L2 resource bounds.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_signature_length",
                &self.max_signature_length,
                "Validates that a transaction has calldata length less than or equal to this \
                 value.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_calldata_length",
                &self.max_calldata_length,
                "Validates that a transaction has signature length less than or equal to this \
                 value.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
pub struct RpcStateReaderConfig {
    pub url: String,
    pub json_rpc_version: String,
}

#[cfg(any(feature = "testing", test))]
impl RpcStateReaderConfig {
    pub fn create_for_testing() -> Self {
        Self { url: "http://localhost:8080".to_string(), json_rpc_version: "2.0".to_string() }
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
