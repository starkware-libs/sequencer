use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use apollo_config::converters::deserialize_milliseconds_to_duration;
use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

const HTTP_SERVER_PORT: u16 = 8080;
pub const DEFAULT_MAX_SIERRA_PROGRAM_SIZE: usize = 4 * 1024 * 1024; // 4MB
// The value is chosen to be much larger than the transaction size limit as enforced by the Starknet
// protocol.
const DEFAULT_MAX_REQUEST_BODY_SIZE: usize = 5 * 1024 * 1024; // 5MB
const DEFAULT_DYNAMIC_CONFIG_POLL_INTERVAL_MS: u64 = 1_000; // 1 second.

/// The http server connection related configuration.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
#[validate(schema(function = "max_size_validations"))]
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
            static_config: HttpServerStaticConfig { ip, port, ..Default::default() },
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
    pub max_request_body_size: usize,
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    pub dynamic_config_poll_interval: Duration,
}

impl SerializeConfig for HttpServerStaticConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param("ip", &self.ip.to_string(), "The http server ip.", ParamPrivacyInput::Public),
            ser_param("port", &self.port, "The http server port.", ParamPrivacyInput::Public),
            ser_param(
                "max_request_body_size",
                &self.max_request_body_size,
                "Max request body size in bytes.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "dynamic_config_poll_interval",
                &self.dynamic_config_poll_interval.as_millis(),
                "Polling interval (in milliseconds) for dynamic config.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl Default for HttpServerStaticConfig {
    fn default() -> Self {
        Self {
            ip: IpAddr::from(Ipv4Addr::UNSPECIFIED),
            port: HTTP_SERVER_PORT,
            max_request_body_size: DEFAULT_MAX_REQUEST_BODY_SIZE,
            dynamic_config_poll_interval: Duration::from_millis(
                DEFAULT_DYNAMIC_CONFIG_POLL_INTERVAL_MS,
            ),
        }
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

fn max_size_validations(http_server_config: &HttpServerConfig) -> Result<(), ValidationError> {
    let max_request_body_size = http_server_config.static_config.max_request_body_size;
    let max_sierra_program_size = http_server_config.dynamic_config.max_sierra_program_size;
    // This validation is not strict, as it does not account for the overhead of the other fields
    // that appear in the request body. On the other hand, this validation compares a limit
    // of the decompressed contract, to the payload limit that stores it compressed.
    if max_request_body_size <= max_sierra_program_size {
        return Err(ValidationError::new(
            "max_request_body_size must be greater than max_sierra_program_size",
        ));
    }
    Ok(())
}
