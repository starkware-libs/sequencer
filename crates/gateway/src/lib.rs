pub mod errors;
pub mod gateway;
pub mod starknet_api_test_utils;
pub mod transaction_validator;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::str::FromStr;

use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use std::net::SocketAddr;
use validator::{Validate, ValidationError};

#[cfg(test)]
mod config_test;

/// The gateway configuration.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct GatewayConfig {
    /// The gateway socket address, in the form of "IP:port".
    #[validate(custom = "validate_socket_address")]
    pub bind_address: String,
}

impl SerializeConfig for GatewayConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([ser_param(
            "server_bind_address",
            &self.bind_address,
            "The server bind addres of a gateway.",
            ParamPrivacyInput::Public,
        )])
    }
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            bind_address: String::from("0.0.0.0:8080"),
        }
    }
}

pub fn validate_socket_address(socket_address: &str) -> Result<(), ValidationError> {
    if SocketAddr::from_str(socket_address).is_err() {
        let mut error = ValidationError::new("Invalid Socket address.");
        error.message = Some("Please provide valid Socket address in the configuration.".into());
        return Err(error);
    }
    Ok(())
}
