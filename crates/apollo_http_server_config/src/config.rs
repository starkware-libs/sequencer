use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};

use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

pub const HTTP_SERVER_PORT: u16 = 8080;
pub const DEFAULT_MAX_SIERRA_PROGRAM_SIZE: usize = 4 * 1024 * 1024; // 4MB

/// The http server connection related configuration.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
pub struct HttpServerConfig {
    pub dynamic_config: HttpServerDynamicConfig,
    pub static_config: HttpServerStaticConfig,
}

impl SerializeConfig for HttpServerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = BTreeMap::new();
        config.extend(prepend_sub_config_name(self.dynamic_config.dump(), "dynamic_config"));
        config.extend(prepend_sub_config_name(self.static_config.dump(), "static_config"));
        config
    }
}

impl HttpServerConfig {
    pub fn new(ip: IpAddr, port: u16, max_sierra_program_size: usize) -> Self {
        Self {
            dynamic_config: HttpServerDynamicConfig {
                accept_new_txs: true,
                max_sierra_program_size,
            },
            static_config: HttpServerStaticConfig { ip, port },
        }
    }

    pub fn ip_and_port(&self) -> (IpAddr, u16) {
        (self.static_config.ip, self.static_config.port)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct HttpServerStaticConfig {
    pub ip: IpAddr,
    pub port: u16,
}

impl SerializeConfig for HttpServerStaticConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param("ip", &self.ip.to_string(), "The http server ip.", ParamPrivacyInput::Public),
            ser_param("port", &self.port, "The http server port.", ParamPrivacyInput::Public),
        ])
    }
}

impl Default for HttpServerStaticConfig {
    fn default() -> Self {
        Self { ip: IpAddr::from(Ipv4Addr::UNSPECIFIED), port: HTTP_SERVER_PORT }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct HttpServerDynamicConfig {
    pub accept_new_txs: bool,
    pub max_sierra_program_size: usize,
}

impl SerializeConfig for HttpServerDynamicConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "accept_new_txs",
                &self.accept_new_txs,
                "Enables accepting new txs.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_sierra_program_size",
                &self.max_sierra_program_size,
                "The maximum size of a sierra program in bytes.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl Default for HttpServerDynamicConfig {
    fn default() -> Self {
        Self { accept_new_txs: true, max_sierra_program_size: DEFAULT_MAX_SIERRA_PROGRAM_SIZE }
    }
}
