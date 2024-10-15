use std::collections::BTreeMap;

use papyrus_config::dumping::{ser_optional_sub_config, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_mempool_infra::component_definitions::{
    LocalComponentCommunicationConfig,
    RemoteClientConfig,
    RemoteServerConfig,
};
use validator::{Validate, ValidationError};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum ComponentExecutionMode {
    Local,
    Remote,
}
// TODO(Lev/Tsabary): When papyrus_config will support it, change to include communication config in
// the enum.

/// The single component configuration.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
#[validate(schema(function = "validate_single_component_config"))]
pub struct ComponentExecutionConfig {
    pub execute: bool,
    pub execution_mode: ComponentExecutionMode,
    pub local_config: Option<LocalComponentCommunicationConfig>,
    pub remote_client_config: Option<RemoteClientConfig>,
    pub remote_server_config: Option<RemoteServerConfig>,
}

impl SerializeConfig for ComponentExecutionConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let config = BTreeMap::from_iter([
            ser_param(
                "execute",
                &self.execute,
                "The component execution flag.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "execution_mode",
                &self.execution_mode,
                "The component execution mode.",
                ParamPrivacyInput::Public,
            ),
        ]);
        vec![
            config,
            ser_optional_sub_config(&self.local_config, "local_config"),
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
            execute: true,
            execution_mode: ComponentExecutionMode::Local,
            local_config: Some(LocalComponentCommunicationConfig::default()),
            remote_client_config: None,
            remote_server_config: None,
        }
    }
}

/// Specific components default configurations.
impl ComponentExecutionConfig {
    pub fn gateway_default_config() -> Self {
        Self {
            execute: true,
            execution_mode: ComponentExecutionMode::Local,
            local_config: Some(LocalComponentCommunicationConfig::default()),
            remote_client_config: None,
            remote_server_config: None,
        }
    }

    // TODO(Tsabary/Lev): There's a bug here: the http server component does not need a local nor a
    // remote config. However, the validation function requires that at least one of them is set. As
    // a workaround I've set the local one, but this should be addressed.
    pub fn http_server_default_config() -> Self {
        Self {
            execute: true,
            execution_mode: ComponentExecutionMode::Remote,
            local_config: None,
            remote_client_config: Some(RemoteClientConfig::default()),
            remote_server_config: None,
        }
    }

    // TODO(Tsabary/Lev): There's a bug here: the monitoring endpoint component does not
    // need a local nor a remote config. However, the validation function requires that at least
    // one of them is set. As a workaround I've set the local one, but this should be addressed.
    pub fn monitoring_endpoint_default_config() -> Self {
        Self {
            execute: true,
            execution_mode: ComponentExecutionMode::Remote,
            local_config: None,
            remote_client_config: Some(RemoteClientConfig::default()),
            remote_server_config: None,
        }
    }

    pub fn mempool_default_config() -> Self {
        Self {
            execute: true,
            execution_mode: ComponentExecutionMode::Local,
            local_config: Some(LocalComponentCommunicationConfig::default()),
            remote_client_config: None,
            remote_server_config: None,
        }
    }

    pub fn batcher_default_config() -> Self {
        Self {
            execute: true,
            execution_mode: ComponentExecutionMode::Local,
            local_config: Some(LocalComponentCommunicationConfig::default()),
            remote_client_config: None,
            remote_server_config: None,
        }
    }

    pub fn consensus_manager_default_config() -> Self {
        Self {
            execute: true,
            execution_mode: ComponentExecutionMode::Local,
            local_config: Some(LocalComponentCommunicationConfig::default()),
            remote_client_config: None,
            remote_server_config: None,
        }
    }
}

pub fn validate_single_component_config(
    component_config: &ComponentExecutionConfig,
) -> Result<(), ValidationError> {
    let error_message = if component_config.execution_mode == ComponentExecutionMode::Local
        && component_config.local_config.is_some()
        && (component_config.remote_server_config.is_some()
            || component_config.remote_client_config.is_some())
    {
        "Local config and Remote config are mutually exclusive in Local mode execution, can't be \
         both active."
    } else if component_config.execution_mode == ComponentExecutionMode::Local
        && component_config.local_config.is_none()
    {
        "Local communication config is missing."
    } else if component_config.execution_mode == ComponentExecutionMode::Remote
        && component_config.remote_server_config.is_none()
        && component_config.remote_client_config.is_none()
    {
        "Remote communication config is missing."
    } else if component_config.execution_mode == ComponentExecutionMode::Remote
        && component_config.remote_server_config.is_some()
        && component_config.remote_client_config.is_some()
    {
        "Remote client and Remote server are mutually exclusive in Remote mode execution, can't be \
         both active."
    } else {
        return Ok(());
    };

    let mut error = ValidationError::new("Invalid component configuration.");
    error.message = Some(error_message.into());
    Err(error)
}
