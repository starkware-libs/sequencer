use std::collections::BTreeMap;
use std::fmt::{Display, Formatter, Result};
use std::net::{IpAddr, Ipv4Addr};
use std::sync::OnceLock;

use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

pub(crate) const DEFAULT_IP: IpAddr = IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0));
pub(crate) const DEFAULT_PORT: u16 = 8082;

/// Global variable set by the main config to enable collecting profiling metrics.
pub static COLLECT_SEQUENCER_PROFILING_METRICS: OnceLock<bool> = OnceLock::new();

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Validate)]
pub struct MonitoringEndpointConfig {
    pub ip: IpAddr,
    pub port: u16,
    pub collect_metrics: bool,
}

impl Default for MonitoringEndpointConfig {
    fn default() -> Self {
        Self { ip: DEFAULT_IP, port: DEFAULT_PORT, collect_metrics: true }
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
            ser_param(
                "collect_metrics",
                &self.collect_metrics,
                "If true, collect and return metrics in the monitoring endpoint.",
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
