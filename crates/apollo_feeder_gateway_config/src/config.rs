use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

const FEEDER_GATEWAY_PORT: u16 = 8082; // configurable; intentionally NOT legacy 9713.

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct FeederGatewayConfig {
    pub ip: IpAddr,
    pub port: u16,
}

impl Default for FeederGatewayConfig {
    fn default() -> Self {
        Self { ip: IpAddr::from(Ipv4Addr::UNSPECIFIED), port: FEEDER_GATEWAY_PORT }
    }
}

impl SerializeConfig for FeederGatewayConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "ip",
                &self.ip.to_string(),
                "The feeder gateway ip.",
                ParamPrivacyInput::Public,
            ),
            ser_param("port", &self.port, "The feeder gateway port.", ParamPrivacyInput::Public),
        ])
    }
}

impl FeederGatewayConfig {
    pub fn ip_and_port(&self) -> (IpAddr, u16) {
        (self.ip, self.port)
    }
}
