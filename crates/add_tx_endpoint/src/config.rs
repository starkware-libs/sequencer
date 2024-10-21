use std::collections::BTreeMap;
use std::net::IpAddr;

use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

/// The http server connection related configuration.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct AddTxEndpointConfig {
    pub ip: IpAddr,
    pub port: u16,
}

impl SerializeConfig for AddTxEndpointConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param("ip", &self.ip.to_string(), "The http server ip.", ParamPrivacyInput::Public),
            ser_param("port", &self.port, "The http server port.", ParamPrivacyInput::Public),
        ])
    }
}

impl Default for AddTxEndpointConfig {
    fn default() -> Self {
        Self { ip: "0.0.0.0".parse().unwrap(), port: 8080 }
    }
}
