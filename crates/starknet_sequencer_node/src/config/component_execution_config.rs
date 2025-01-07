use std::collections::BTreeMap;
#[cfg(any(feature = "testing", test))]
use std::net::SocketAddr;

use papyrus_config::dumping::{append_sub_config_name, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_sequencer_infra::component_definitions::{
    LocalServerConfig,
    RemoteClientConfig,
    RemoteServerConfig,
};
use validator::{Validate, ValidationError};

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
    pub remote_server_config: RemoteServerConfig,
}

impl SerializeConfig for ReactiveComponentExecutionConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let members = BTreeMap::from_iter([ser_param(
            "execution_mode",
            &self.execution_mode,
            "The component execution mode.",
            ParamPrivacyInput::Public,
        )]);
        vec![
            members,
            append_sub_config_name(self.local_server_config.dump(), "local_server_config"),
            append_sub_config_name(self.remote_client_config.dump(), "remote_client_config"),
            append_sub_config_name(self.remote_server_config.dump(), "remote_server_config"),
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
            remote_server_config: RemoteServerConfig::default(),
        }
    }

    pub fn remote(socket: SocketAddr) -> Self {
        Self {
            execution_mode: ReactiveComponentExecutionMode::Remote,
            local_server_config: LocalServerConfig::default(),
            remote_client_config: RemoteClientConfig { socket, ..RemoteClientConfig::default() },
            remote_server_config: RemoteServerConfig::default(),
        }
    }

    pub fn local_with_remote_enabled(socket: SocketAddr) -> Self {
        Self {
            execution_mode: ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled,
            local_server_config: LocalServerConfig::default(),
            remote_client_config: RemoteClientConfig { socket, ..RemoteClientConfig::default() },
            remote_server_config: RemoteServerConfig { socket },
        }
    }

    pub fn local_with_remote_disabled_for_testing(socket: SocketAddr) -> Self {
        Self {
            execution_mode: ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled,
            local_server_config: LocalServerConfig::default(),
            remote_client_config: RemoteClientConfig { socket, ..RemoteClientConfig::default() },
            remote_server_config: RemoteServerConfig { socket },
        }
    }
}

impl ReactiveComponentExecutionConfig {
    pub fn local_with_remote_disabled() -> Self {
        Self {
            execution_mode: ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled,
            local_server_config: LocalServerConfig::default(),
            remote_client_config: RemoteClientConfig::default(),
            remote_server_config: RemoteServerConfig::default(),
        }
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

fn validate_reactive_component_execution_config(
    component_config: &ReactiveComponentExecutionConfig,
) -> Result<(), ValidationError> {
    match component_config.execution_mode {
        ReactiveComponentExecutionMode::Remote
        | ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            if component_config.remote_client_config.socket.port() == 0
                || component_config.remote_server_config.socket.port() == 0
            {
                let mut error =
                    ValidationError::new("Invalid reactive component execution configuration.");
                error.message = Some(
                    "Socket is unspecified on remote connection. Ensure settings align with the \
                     chosen execution mode."
                        .into(),
                );
                return Err(error);
            }
            Ok(())
        }
        _ => Ok(()),
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
