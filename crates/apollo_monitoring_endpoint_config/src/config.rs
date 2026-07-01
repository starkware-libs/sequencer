use std::fmt::{Display, Formatter, Result};
use std::net::{IpAddr, Ipv4Addr};

use serde::{Deserialize, Serialize};
use validator::Validate;

pub const MONITORING_ENDPOINT_DEFAULT_IP: IpAddr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
pub const MONITORING_ENDPOINT_DEFAULT_PORT: u16 = 8082;
pub const MONITORING_ENDPOINT_DEFAULT_SNAPSHOT_TIMEOUT_MILLIS: u64 = 5000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Validate)]
pub struct MonitoringEndpointConfig {
    pub ip: IpAddr,
    pub port: u16,
    /// Timeout in milliseconds for snapshot requests to internal services (mempool, L1 provider).
    pub snapshot_timeout_millis: u64,
}

impl MonitoringEndpointConfig {
    pub fn deployment() -> Self {
        Self {
            ip: MONITORING_ENDPOINT_DEFAULT_IP,
            port: MONITORING_ENDPOINT_DEFAULT_PORT,
            snapshot_timeout_millis: MONITORING_ENDPOINT_DEFAULT_SNAPSHOT_TIMEOUT_MILLIS,
        }
    }
}

impl Default for MonitoringEndpointConfig {
    fn default() -> Self {
        MonitoringEndpointConfig::deployment()
    }
}

impl Display for MonitoringEndpointConfig {
    #[cfg_attr(coverage_nightly, coverage_attribute)]
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{self:?}")
    }
}
