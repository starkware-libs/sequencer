use std::collections::BTreeMap;

use papyrus_config::dumping::{
    append_sub_config_name,
    ser_optional_sub_config,
    ser_param,
    SerializeConfig,
};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_sequencer_infra::component_definitions::{
    LocalClientConfig,
    LocalServerConfig,
    RemoteClientConfig,
    RemoteServerConfig,
};
use validator::{Validate, ValidationError};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum ComponentExecutionMode {
    Disabled,
    LocalExecution { enable_remote_connection: bool },
}

impl ComponentExecutionMode {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        match self {
            ComponentExecutionMode::Disabled => BTreeMap::from_iter([ser_param(
                "Disabled",
                &"Disabled",
                "The component is disabled.",
                ParamPrivacyInput::Public,
            )]),
            ComponentExecutionMode::LocalExecution { enable_remote_connection } => {
                BTreeMap::from_iter([ser_param(
                    "LocalExecution.enable_remote_connection",
                    enable_remote_connection,
                    "Specifies whether the component, when running locally, allows remote \
                     connections.",
                    ParamPrivacyInput::Public,
                )])
            }
        }
    }
}
// TODO(Lev/Tsabary): When papyrus_config will support it, change to include communication config in
// the enum.

/// The single component configuration.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
#[validate(schema(function = "validate_single_component_config"))]
pub struct ComponentExecutionConfig {
    pub execution_mode: ComponentExecutionMode,
    pub local_client_config: Option<LocalClientConfig>,
    pub local_server_config: Option<LocalServerConfig>,
    pub remote_client_config: Option<RemoteClientConfig>,
    pub remote_server_config: Option<RemoteServerConfig>,
}

impl SerializeConfig for ComponentExecutionConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        vec![
            append_sub_config_name(self.execution_mode.dump(), "execution_mode"),
            ser_optional_sub_config(&self.local_server_config, "local_server_config"),
            ser_optional_sub_config(&self.remote_client_config, "remote_client_config"),
            ser_optional_sub_config(&self.remote_server_config, "remote_server_config"),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

impl Default for ComponentExecutionConfig {
    fn default() -> Self {
        Self {
            execution_mode: ComponentExecutionMode::LocalExecution {
                enable_remote_connection: false,
            },
            local_server_config: Some(LocalServerConfig::default()),
            local_client_config: None,
            remote_client_config: None,
            remote_server_config: None,
        }
    }
}

/// Specific components default configurations.
impl ComponentExecutionConfig {
    pub fn gateway_default_config() -> Self {
        Self {
            execution_mode: ComponentExecutionMode::LocalExecution {
                enable_remote_connection: false,
            },
            local_server_config: Some(LocalServerConfig::default()),
            local_client_config: None,
            remote_client_config: None,
            remote_server_config: None,
        }
    }

    // TODO(Tsabary/Lev): There's a bug here: the http server component does not need a local nor a
    // remote config. However, the validation function requires that at least one of them is set. As
    // a workaround I've set the local one, but this should be addressed.
    pub fn http_server_default_config() -> Self {
        Self {
            execution_mode: ComponentExecutionMode::LocalExecution {
                enable_remote_connection: true,
            },
            local_server_config: None,
            local_client_config: None,
            remote_client_config: Some(RemoteClientConfig::default()),
            remote_server_config: None,
        }
    }

    // TODO(Tsabary/Lev): There's a bug here: the monitoring endpoint component does not
    // need a local nor a remote config. However, the validation function requires that at least
    // one of them is set. As a workaround I've set the local one, but this should be addressed.
    pub fn monitoring_endpoint_default_config() -> Self {
        Self {
            execution_mode: ComponentExecutionMode::LocalExecution {
                enable_remote_connection: true,
            },
            local_server_config: None,
            local_client_config: None,
            remote_client_config: Some(RemoteClientConfig::default()),
            remote_server_config: None,
        }
    }

    pub fn mempool_default_config() -> Self {
        Self {
            execution_mode: ComponentExecutionMode::LocalExecution {
                enable_remote_connection: false,
            },
            local_server_config: Some(LocalServerConfig::default()),
            local_client_config: None,
            remote_client_config: None,
            remote_server_config: None,
        }
    }

    pub fn batcher_default_config() -> Self {
        Self {
            execution_mode: ComponentExecutionMode::LocalExecution {
                enable_remote_connection: false,
            },
            local_server_config: Some(LocalServerConfig::default()),
            local_client_config: None,
            remote_client_config: None,
            remote_server_config: None,
        }
    }

    pub fn consensus_manager_default_config() -> Self {
        Self {
            execution_mode: ComponentExecutionMode::LocalExecution {
                enable_remote_connection: false,
            },
            local_server_config: Some(LocalServerConfig::default()),
            local_client_config: None,
            remote_client_config: None,
            remote_server_config: None,
        }
    }

    pub fn mempool_p2p_default_config() -> Self {
        Self {
            execution_mode: ComponentExecutionMode::LocalExecution {
                enable_remote_connection: false,
            },
            local_server_config: Some(LocalServerConfig::default()),
            local_client_config: None,
            remote_client_config: None,
            remote_server_config: None,
        }
    }
}

pub fn validate_single_component_config(
    component_config: &ComponentExecutionConfig,
) -> Result<(), ValidationError> {
    let local_execution_mode =
        ComponentExecutionMode::LocalExecution { enable_remote_connection: false };
    let enable_remote_execution_mode =
        ComponentExecutionMode::LocalExecution { enable_remote_connection: true };
    let disabled = ComponentExecutionMode::Disabled;

    let error_message = match (
        component_config.execution_mode.clone(),
        component_config.local_server_config.is_some(),
        component_config.remote_server_config.is_some(),
        component_config.remote_client_config.is_some(),
    ) {
        (mode, _, true, true) if mode == enable_remote_execution_mode => {
            "Remote server and remote client are mutually exclusive; both cannot be active \
             simultaneously."
        }
        (mode, true, _, true) if mode == enable_remote_execution_mode => {
            "Local server and remote client are mutually exclusive; both cannot be active \
             simultaneously."
        }
        (mode, _, false, false) if mode == enable_remote_execution_mode => {
            "Remote communication config is missing."
        }
        (mode, false, _, _) if mode == local_execution_mode => "Local server config is missing.",
        (mode, true, true, _) if mode == local_execution_mode => {
            "Cannot provide remote server configuration in local execution mode with remote \
             connection disabled."
        }
        (mode, true, _, true) if mode == local_execution_mode => {
            "Cannot provide remote client configuration in local execution mode with remote \
             connection disabled."
        }
        (mode, true, _, _) if mode == disabled => {
            "Configuration cannot be provided in disabled execution mode."
        }
        (mode, _, true, _) if mode == disabled => {
            "Configuration cannot be provided in disabled execution mode."
        }
        (mode, _, _, true) if mode == disabled => {
            "Configuration cannot be provided in disabled execution mode."
        }
        _ => return Ok(()),
    };

    let mut error = ValidationError::new("Invalid component configuration.");
    error.message = Some(error_message.into());
    Err(error)
}
