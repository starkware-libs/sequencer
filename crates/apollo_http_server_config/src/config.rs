use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

pub const HTTP_SERVER_PORT: u16 = 8080;
pub const DEFAULT_MAX_SIERRA_PROGRAM_SIZE: usize = 4 * 1024 * 1024; // 4MB

/// The http server connection related configuration.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct HttpServerConfig {
    pub ip: IpAddr,
    pub port: u16,
    pub max_sierra_program_size: usize,
}

impl SerializeConfig for HttpServerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param("ip", &self.ip.to_string(), "The http server ip.", ParamPrivacyInput::Public),
            ser_param("port", &self.port, "The http server port.", ParamPrivacyInput::Public),
            ser_param(
                "max_sierra_program_size",
                &self.max_sierra_program_size,
                "The maximum size of a sierra program in bytes.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl Default for HttpServerConfig {
    fn default() -> Self {
        Self {
            ip: IpAddr::from(Ipv4Addr::UNSPECIFIED),
            port: HTTP_SERVER_PORT,
            max_sierra_program_size: DEFAULT_MAX_SIERRA_PROGRAM_SIZE,
        }
    }
}
