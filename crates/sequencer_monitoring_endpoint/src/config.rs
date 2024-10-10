use std::collections::BTreeMap;
use std::fmt::Display;
use std::net::IpAddr;

use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Validate)]
pub struct SequencerMonitoringEndpointConfig {
    pub ip: IpAddr,
    pub port: u16,
}

impl Default for SequencerMonitoringEndpointConfig {
    fn default() -> Self {
        Self { ip: "0.0.0.0".parse().unwrap(), port: 8082 }
    }
}

impl SerializeConfig for SequencerMonitoringEndpointConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "ip",
                &self.ip.to_string(),
                "The sequenser monitoring endpoint ip address.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "port",
                &self.port,
                "The sequenser monitoring endpoint port.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl Display for SequencerMonitoringEndpointConfig {
    #[cfg_attr(coverage_nightly, coverage_attribute)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
