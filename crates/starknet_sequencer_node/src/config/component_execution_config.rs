use std::collections::BTreeMap;

use papyrus_config::dumping::{ser_optional_sub_config, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_sequencer_infra::component_definitions::{
    LocalServerConfig,
    RemoteClientConfig,
    RemoteServerConfig,
};
use tracing::error;
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
    pub local_server_config: Option<LocalServerConfig>,
    pub remote_client_config: Option<RemoteClientConfig>,
    pub remote_server_config: Option<RemoteServerConfig>,
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
            ser_optional_sub_config(&self.local_server_config, "local_server_config"),
            ser_optional_sub_config(&self.remote_client_config, "remote_client_config"),
            ser_optional_sub_config(&self.remote_server_config, "remote_server_config"),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

impl Default for ReactiveComponentExecutionConfig {
    fn default() -> Self {
        Self {
            execution_mode: ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled,
            local_server_config: Some(LocalServerConfig::default()),
            remote_client_config: None,
            remote_server_config: None,
        }
    }
}

/// Active component configuration.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct ActiveComponentExecutionConfig {
    pub execution_mode: ActiveComponentExecutionMode,
    pub remote_client_config: Option<RemoteClientConfig>,
}

impl SerializeConfig for ActiveComponentExecutionConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let members = BTreeMap::from_iter([ser_param(
            "execution_mode",
            &self.execution_mode,
            "The component execution mode.",
            ParamPrivacyInput::Public,
        )]);
        vec![members, ser_optional_sub_config(&self.remote_client_config, "remote_client_config")]
            .into_iter()
            .flatten()
            .collect()
    }
}

impl Default for ActiveComponentExecutionConfig {
    fn default() -> Self {
        Self { execution_mode: ActiveComponentExecutionMode::Enabled, remote_client_config: None }
    }
}

/// Specific components default configurations.
impl ReactiveComponentExecutionConfig {
    pub fn gateway_default_config() -> Self {
        Self {
            execution_mode: ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled,
            local_server_config: Some(LocalServerConfig::default()),
            remote_client_config: None,
            remote_server_config: None,
        }
    }

    // TODO(Tsabary/Lev): There's a bug here: the http server component does not need a local nor a
    // remote config. However, the validation function requires that at least one of them is set. As
    // a workaround I've set the local one, but this should be addressed.
    pub fn http_server_default_config() -> Self {
        Self {
            execution_mode: ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled,
            local_server_config: Some(LocalServerConfig::default()),
            remote_client_config: None,
            remote_server_config: Some(RemoteServerConfig::default()),
        }
    }

    pub fn mempool_default_config() -> Self {
        Self {
            execution_mode: ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled,
            local_server_config: Some(LocalServerConfig::default()),
            remote_client_config: None,
            remote_server_config: None,
        }
    }

    pub fn batcher_default_config() -> Self {
        Self {
            execution_mode: ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled,
            local_server_config: Some(LocalServerConfig::default()),
            remote_client_config: None,
            remote_server_config: None,
        }
    }

    pub fn consensus_manager_default_config() -> Self {
        Self {
            execution_mode: ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled,
            local_server_config: Some(LocalServerConfig::default()),
            remote_client_config: None,
            remote_server_config: None,
        }
    }

    pub fn mempool_p2p_default_config() -> Self {
        Self {
            execution_mode: ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled,
            local_server_config: Some(LocalServerConfig::default()),
            remote_client_config: None,
            remote_server_config: None,
        }
    }

    pub fn state_sync_default_config() -> Self {
        Self {
            execution_mode: ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled,
            local_server_config: Some(LocalServerConfig::default()),
            remote_client_config: None,
            remote_server_config: None,
        }
    }
}

fn validate_reactive_component_execution_config(
    component_config: &ReactiveComponentExecutionConfig,
) -> Result<(), ValidationError> {
    match (
        component_config.execution_mode.clone(),
        component_config.local_server_config.is_some(),
        component_config.remote_client_config.is_some(),
        component_config.remote_server_config.is_some(),
    ) {
        (ReactiveComponentExecutionMode::Disabled, false, false, false) => Ok(()),
        (ReactiveComponentExecutionMode::Remote, false, true, false) => Ok(()),
        (ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled, true, false, true) => {
            Ok(())
        }
        (ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled, true, false, false) => {
            Ok(())
        }
        (mode, local_server_config, remote_client_config, remote_server_config) => {
            error!(
                "Invalid component execution configuration: mode: {:?}, local_server_config: \
                 {:?}, remote_client_config: {:?}, remote_server_config: {:?}",
                mode, local_server_config, remote_client_config, remote_server_config
            );
            let mut error = ValidationError::new("Invalid component execution configuration.");
            error.message = Some("Ensure settings align with the chosen execution mode.".into());
            Err(error)
        }
    }
}
