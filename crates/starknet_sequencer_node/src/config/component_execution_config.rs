use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, ToSocketAddrs};

use papyrus_config::dumping::{append_sub_config_name, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_sequencer_infra::component_definitions::{LocalServerConfig, RemoteClientConfig};
use tracing::error;
use validator::{Validate, ValidationError};

const DEFAULT_URL: &str = "localhost";
const DEFAULT_IP: IpAddr = IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0));
const DEFAULT_INVALID_PORT: u16 = 0;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum ReactiveComponentExecutionMode {
    Disabled,
    Remote,
    LocalExecutionWithRemoteEnabled,
    LocalExecutionWithRemoteDisabled,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum ActiveComponentExecutionMode {
    Disabled,
    Enabled,
}

// TODO(Lev/Tsabary): When papyrus_config will support it, change to include communication config in
// the enum.

/// Reactive component configuration.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
#[validate(schema(function = "validate_reactive_component_execution_config"))]
pub struct ReactiveComponentExecutionConfig {
    pub execution_mode: ReactiveComponentExecutionMode,
    pub local_server_config: LocalServerConfig,
    pub remote_client_config: RemoteClientConfig,
    pub url: String,
    pub ip: IpAddr,
    pub port: u16,
}

impl SerializeConfig for ReactiveComponentExecutionConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let members = BTreeMap::from_iter([
            ser_param(
                "execution_mode",
                &self.execution_mode,
                "The component execution mode.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "url",
                &self.url,
                "URL of the remote component server.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "ip",
                &self.ip.to_string(),
                "Binding address of the remote component server.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "port",
                &self.port,
                "Listening port of the remote component server.",
                ParamPrivacyInput::Public,
            ),
        ]);
        vec![
            members,
            append_sub_config_name(self.local_server_config.dump(), "local_server_config"),
            append_sub_config_name(self.remote_client_config.dump(), "remote_client_config"),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

impl Default for ReactiveComponentExecutionConfig {
    fn default() -> Self {
        Self::local_with_remote_disabled()
    }
}

#[cfg(any(feature = "testing", test))]
impl ReactiveComponentExecutionConfig {
    pub fn disabled() -> Self {
        Self {
            execution_mode: ReactiveComponentExecutionMode::Disabled,
            local_server_config: LocalServerConfig::default(),
            remote_client_config: RemoteClientConfig::default(),
            url: DEFAULT_URL.to_string(),
            ip: DEFAULT_IP,
            port: DEFAULT_INVALID_PORT,
        }
    }

    pub fn remote(url: String, ip: IpAddr, port: u16) -> Self {
        Self {
            execution_mode: ReactiveComponentExecutionMode::Remote,
            local_server_config: LocalServerConfig::default(),
            remote_client_config: RemoteClientConfig::default(),
            url,
            ip,
            port,
        }
    }

    pub fn local_with_remote_enabled(url: String, ip: IpAddr, port: u16) -> Self {
        Self {
            execution_mode: ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled,
            local_server_config: LocalServerConfig::default(),
            remote_client_config: RemoteClientConfig::default(),
            url,
            ip,
            port,
        }
    }
}

impl ReactiveComponentExecutionConfig {
    pub fn local_with_remote_disabled() -> Self {
        Self {
            execution_mode: ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled,
            local_server_config: LocalServerConfig::default(),
            remote_client_config: RemoteClientConfig::default(),
            url: DEFAULT_URL.to_string(),
            ip: DEFAULT_IP,
            port: DEFAULT_INVALID_PORT,
        }
    }
    fn is_valid_socket(&self) -> bool {
        self.port != 0
    }
}

/// Active component configuration.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
#[validate(schema(function = "validate_active_component_execution_config"))]
pub struct ActiveComponentExecutionConfig {
    pub execution_mode: ActiveComponentExecutionMode,
}

impl SerializeConfig for ActiveComponentExecutionConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([ser_param(
            "execution_mode",
            &self.execution_mode,
            "The component execution mode.",
            ParamPrivacyInput::Public,
        )])
    }
}

impl Default for ActiveComponentExecutionConfig {
    fn default() -> Self {
        ActiveComponentExecutionConfig::enabled()
    }
}

impl ActiveComponentExecutionConfig {
    pub fn disabled() -> Self {
        Self { execution_mode: ActiveComponentExecutionMode::Disabled }
    }

    pub fn enabled() -> Self {
        Self { execution_mode: ActiveComponentExecutionMode::Enabled }
    }
}

// This function checks that the domain could be resolved to an IP address. If it fails, it returns
// an error.
fn check_domain(domain: &str) -> Result<(), String> {
    let socket_addrs = (domain, 0).to_socket_addrs().map_err(|e| e.to_string())?;
    if socket_addrs.count() > 0 {
        Ok(())
    } else {
        Err("No IP address found for the domain".to_string())
    }
}

// This function validates the configured URL. If the URL is invalid, it returns an error.
fn validate_url(url: &str) -> Result<(), ValidationError> {
    match check_domain(url) {
        Ok(_) => Ok(()),
        Err(e) => {
            error!("Failed to resolve url IP: {}", e);
            let mut error = ValidationError::new("Failed to resolve url IP");
            error.message = Some("Ensure the url is valid.".into());
            Err(error)
        }
    }
}

fn validate_reactive_component_execution_config(
    component_config: &ReactiveComponentExecutionConfig,
) -> Result<(), ValidationError> {
    match (component_config.execution_mode.clone(), component_config.is_valid_socket()) {
        (ReactiveComponentExecutionMode::Disabled, _) => Ok(()),
        (ReactiveComponentExecutionMode::Remote, true) => {
            validate_url(component_config.url.as_str())
        }
        (ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled, true) => {
            validate_url(component_config.url.as_str())
        }
        (ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled, _) => Ok(()),
        (mode, socket) => {
            error!(
                "Invalid reactive component execution configuration: mode: {:?}, socket: {:?}",
                mode, socket
            );
            let mut error =
                ValidationError::new("Invalid reactive component execution configuration.");
            error.message = Some("Ensure settings align with the chosen execution mode.".into());
            Err(error)
        }
    }
}

fn validate_active_component_execution_config(
    _component_config: &ActiveComponentExecutionConfig,
) -> Result<(), ValidationError> {
    Ok(())
}

// There are components that are described with a reactive mode setting, however, result in the
// creation of two components: one reactive and one active. The defined behavior is such that
// the active component is active if and only if the local component is running locally. The
// following function applies this logic.
impl From<ReactiveComponentExecutionMode> for ActiveComponentExecutionMode {
    fn from(mode: ReactiveComponentExecutionMode) -> Self {
        match mode {
            ReactiveComponentExecutionMode::Disabled | ReactiveComponentExecutionMode::Remote => {
                ActiveComponentExecutionMode::Disabled
            }
            ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled
            | ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled => {
                ActiveComponentExecutionMode::Enabled
            }
        }
    }
}
