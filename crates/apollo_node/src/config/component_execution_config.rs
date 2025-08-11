use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, ToSocketAddrs};

use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_infra::component_client::RemoteClientConfig;
use apollo_infra::component_server::LocalServerConfig;
use serde::{Deserialize, Serialize};
use tracing::error;
use validator::{Validate, ValidationError};

use crate::config::config_utils::create_validation_error;

const DEFAULT_URL: &str = "localhost";
const DEFAULT_IP: IpAddr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
const DEFAULT_INVALID_PORT: u16 = 0;

pub const MAX_CONCURRENCY: usize = 8;

pub trait ExpectedComponentConfig {
    fn is_component_config_expected(&self) -> bool;
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum ReactiveComponentExecutionMode {
    Disabled,
    Remote,
    LocalExecutionWithRemoteEnabled,
    LocalExecutionWithRemoteDisabled,
}

impl ExpectedComponentConfig for ReactiveComponentExecutionMode {
    fn is_component_config_expected(&self) -> bool {
        match self {
            ReactiveComponentExecutionMode::Disabled | ReactiveComponentExecutionMode::Remote => {
                false
            }
            ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled
            | ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled => true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum ActiveComponentExecutionMode {
    Disabled,
    Enabled,
}

impl ExpectedComponentConfig for ActiveComponentExecutionMode {
    fn is_component_config_expected(&self) -> bool {
        match self {
            ActiveComponentExecutionMode::Disabled => false,
            ActiveComponentExecutionMode::Enabled => true,
        }
    }
}

/// Reactive component configuration.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
#[validate(schema(function = "validate_reactive_component_execution_config"))]
pub struct ReactiveComponentExecutionConfig {
    pub execution_mode: ReactiveComponentExecutionMode,
    pub local_server_config: LocalServerConfig,
    pub remote_client_config: RemoteClientConfig,
    #[validate(custom = "validate_max_concurrency")]
    pub max_concurrency: usize,
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
                "max_concurrency",
                &self.max_concurrency,
                "The maximum number of concurrent requests handling.",
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
            prepend_sub_config_name(self.local_server_config.dump(), "local_server_config"),
            prepend_sub_config_name(self.remote_client_config.dump(), "remote_client_config"),
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

impl ReactiveComponentExecutionConfig {
    pub fn disabled() -> Self {
        Self {
            execution_mode: ReactiveComponentExecutionMode::Disabled,
            local_server_config: LocalServerConfig::default(),
            remote_client_config: RemoteClientConfig::default(),
            max_concurrency: MAX_CONCURRENCY,
            url: DEFAULT_URL.to_string(),
            ip: DEFAULT_IP,
            port: DEFAULT_INVALID_PORT,
        }
    }

    pub fn remote(url: String, ip: IpAddr, port: u16) -> Self {
        Self {
            execution_mode: ReactiveComponentExecutionMode::Remote,
            local_server_config: LocalServerConfig::default(),
            max_concurrency: MAX_CONCURRENCY,
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
            max_concurrency: MAX_CONCURRENCY,
            url,
            ip,
            port,
        }
    }

    pub fn local_with_remote_disabled() -> Self {
        Self {
            execution_mode: ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled,
            local_server_config: LocalServerConfig::default(),
            remote_client_config: RemoteClientConfig::default(),
            max_concurrency: MAX_CONCURRENCY,
            url: DEFAULT_URL.to_string(),
            ip: DEFAULT_IP,
            port: DEFAULT_INVALID_PORT,
        }
    }
    fn is_valid_socket(&self) -> bool {
        self.port != 0
    }

    #[cfg(any(feature = "testing", test))]
    pub fn set_url_to_localhost(&mut self) {
        self.url = Ipv4Addr::LOCALHOST.to_string();
    }
}

impl ExpectedComponentConfig for ReactiveComponentExecutionConfig {
    fn is_component_config_expected(&self) -> bool {
        self.execution_mode.is_component_config_expected()
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

impl ExpectedComponentConfig for ActiveComponentExecutionConfig {
    fn is_component_config_expected(&self) -> bool {
        self.execution_mode.is_component_config_expected()
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

// Validates the configured URL. If the URL is invalid, it returns an error.
fn validate_url(url: &str) -> Result<(), ValidationError> {
    let arbitrary_port: u16 = 0;
    let socket_addrs = (url, arbitrary_port)
        .to_socket_addrs()
        .map_err(|e| create_url_validation_error(format!("Failed to resolve url IP: {e}")))?;

    if socket_addrs.count() > 0 {
        Ok(())
    } else {
        Err(create_url_validation_error("No IP address found for the domain".to_string()))
    }
}

fn create_url_validation_error(error_msg: String) -> ValidationError {
    create_validation_error(error_msg, "Failed to resolve url IP", "Ensure the url is valid.")
}

// Validate the configured max concurrency. If the max concurrency is invalid, it returns an error.
fn validate_max_concurrency(max_concurrency: usize) -> Result<(), ValidationError> {
    if max_concurrency > 0 {
        Ok(())
    } else {
        Err(create_validation_error(
            format!("Invalid max_concurrency: {max_concurrency}"),
            "Invalid max concurrency",
            "Ensure the max concurrency is greater than 0.",
        ))
    }
}

fn validate_reactive_component_execution_config(
    component_config: &ReactiveComponentExecutionConfig,
) -> Result<(), ValidationError> {
    match (&component_config.execution_mode, component_config.is_valid_socket()) {
        (ReactiveComponentExecutionMode::Disabled, _) => Ok(()),
        (ReactiveComponentExecutionMode::Remote, true)
        | (ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled, true) => {
            validate_url(&component_config.url)
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
