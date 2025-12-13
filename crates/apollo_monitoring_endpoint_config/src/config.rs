use std::collections::BTreeMap;
use std::fmt::{Display, Formatter, Result};
use std::net::{IpAddr, Ipv4Addr};

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

pub const MONITORING_ENDPOINT_DEFAULT_IP: IpAddr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
pub const MONITORING_ENDPOINT_DEFAULT_PORT: u16 = 8082;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Validate)]
pub struct MonitoringEndpointConfig {
    pub ip: IpAddr,
    pub port: u16,
}

impl MonitoringEndpointConfig {
    pub fn deployment() -> Self {
        Self { ip: MONITORING_ENDPOINT_DEFAULT_IP, port: MONITORING_ENDPOINT_DEFAULT_PORT }
    }
}

impl Default for MonitoringEndpointConfig {
    fn default() -> Self {
        MonitoringEndpointConfig::deployment()
    }
}

impl SerializeConfig for MonitoringEndpointConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "ip",
                &self.ip.to_string(),
                "The monitoring endpoint ip address.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "port",
                &self.port,
                "The monitoring endpoint port.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl Display for MonitoringEndpointConfig {
    #[cfg_attr(coverage_nightly, coverage_attribute)]
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{self:?}")
    }
}
