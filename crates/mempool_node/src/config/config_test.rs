use std::env;
use std::fs::File;

use assert_json_diff::assert_json_eq;
use assert_matches::assert_matches;
use colored::Colorize;
use mempool_test_utils::get_absolute_path;
use papyrus_config::dumping::SerializeConfig;
use papyrus_config::validators::{ParsedValidationError, ParsedValidationErrors};
use rstest::rstest;
use starknet_mempool_infra::component_definitions::{
    LocalComponentCommunicationConfig,
    RemoteClientConfig,
    RemoteServerConfig,
};
use validator::{Validate, ValidationErrors};

use crate::config::{
    ComponentConfig,
    ComponentExecutionConfig,
    ComponentExecutionMode,
    SequencerNodeConfig,
    CONFIG_POINTERS,
    DEFAULT_CONFIG_PATH,
};

fn check_validation_error(
    validation_result: Result<(), ValidationErrors>,
    code_str: &str,
    message_str: &str,
) {
    assert_matches!(validation_result.unwrap_err(), validation_errors => {
        let parsed_errors = ParsedValidationErrors::from(validation_errors);
        assert_eq!(parsed_errors.0.len(), 1);
        let parsed_validation_error = &parsed_errors.0[0];
        assert_matches!(
            parsed_validation_error,
            ParsedValidationError { param_path, code, message, params}
            if (
                param_path == "__all__" &&
                code == code_str &&
                params.is_empty() &&
                *message == Some(message_str.to_string())
            )
        )
    });
}

/// Test the validation of the struct ComponentExecutionConfig.
/// Validates that execution mode of the component and the local/remote config are at sync.
#[rstest]
#[case(
    ComponentExecutionMode::Local,
    Some(LocalComponentCommunicationConfig::default()),
    Some(RemoteClientConfig::default()),
    Some(RemoteServerConfig::default()),
    "Local config and Remote config are mutually exclusive in Local mode execution, can't be both \
     active."
)]
#[case(
    ComponentExecutionMode::Local,
    Some(LocalComponentCommunicationConfig::default()),
    None,
    Some(RemoteServerConfig::default()),
    "Local config and Remote config are mutually exclusive in Local mode execution, can't be both \
     active."
)]
#[case(
    ComponentExecutionMode::Local,
    Some(LocalComponentCommunicationConfig::default()),
    Some(RemoteClientConfig::default()),
    None,
    "Local config and Remote config are mutually exclusive in Local mode execution, can't be both \
     active."
)]
#[case(
    ComponentExecutionMode::Local,
    None,
    Some(RemoteClientConfig::default()),
    Some(RemoteServerConfig::default()),
    "Local communication config is missing."
)]
#[case(
    ComponentExecutionMode::Local,
    None,
    None,
    Some(RemoteServerConfig::default()),
    "Local communication config is missing."
)]
#[case(
    ComponentExecutionMode::Local,
    None,
    Some(RemoteClientConfig::default()),
    None,
    "Local communication config is missing."
)]
#[case(ComponentExecutionMode::Local, None, None, None, "Local communication config is missing.")]
#[case(
    ComponentExecutionMode::Remote,
    Some(LocalComponentCommunicationConfig::default()),
    None,
    None,
    "Remote communication config is missing."
)]
#[case(
    ComponentExecutionMode::Remote,
    None,
    Some(RemoteClientConfig::default()),
    Some(RemoteServerConfig::default()),
    "Remote client and Remote server are mutually exclusive in Remote mode execution, can't be \
     both active."
)]
#[case(
    ComponentExecutionMode::Remote,
    Some(LocalComponentCommunicationConfig::default()),
    Some(RemoteClientConfig::default()),
    Some(RemoteServerConfig::default()),
    "Remote client and Remote server are mutually exclusive in Remote mode execution, can't be \
     both active."
)]
#[case(ComponentExecutionMode::Remote, None, None, None, "Remote communication config is missing.")]
fn test_invalid_component_execution_config(
    #[case] execution_mode: ComponentExecutionMode,
    #[case] local_config: Option<LocalComponentCommunicationConfig>,
    #[case] remote_client_config: Option<RemoteClientConfig>,
    #[case] remote_server_config: Option<RemoteServerConfig>,
    #[case] expected_error_message: &str,
) {
    // Initialize an invalid config and check that the validator finds an error.
    let component_exe_config = ComponentExecutionConfig {
        execution_mode,
        local_config,
        remote_client_config,
        remote_server_config,
        ..ComponentExecutionConfig::default()
    };
    check_validation_error(
        component_exe_config.validate(),
        "Invalid component configuration.",
        expected_error_message,
    );
}

/// Test the validation of the struct ComponentExecutionConfig.
/// Validates that execution mode of the component and the local/remote config are at sync.
#[rstest]
#[case::local(ComponentExecutionMode::Local, None, None)]
#[case::remote(ComponentExecutionMode::Remote, Some(RemoteClientConfig::default()), None)]
#[case::remote(ComponentExecutionMode::Remote, None, Some(RemoteServerConfig::default()))]
fn test_valid_component_execution_config(
    #[case] execution_mode: ComponentExecutionMode,
    #[case] remote_client_config: Option<RemoteClientConfig>,
    #[case] remote_server_config: Option<RemoteServerConfig>,
) {
    // Initialize a valid config and check that the validator returns Ok.

    let local_config = if execution_mode == ComponentExecutionMode::Local {
        Some(LocalComponentCommunicationConfig::default())
    } else {
        None
    };

    let component_exe_config = ComponentExecutionConfig {
        execution_mode,
        local_config,
        remote_client_config,
        remote_server_config,
        ..ComponentExecutionConfig::default()
    };
    assert_eq!(component_exe_config.validate(), Ok(()));
}

#[test]
fn test_invalid_components_config() {
    // Initialize an invalid config and check that the validator finds an error.
    let component_config = ComponentConfig {
        batcher: ComponentExecutionConfig { execute: false, ..ComponentExecutionConfig::default() },
        consensus_manager: ComponentExecutionConfig {
            execute: false,
            ..ComponentExecutionConfig::default()
        },
        gateway: ComponentExecutionConfig { execute: false, ..ComponentExecutionConfig::default() },
        http_server: ComponentExecutionConfig {
            execute: false,
            ..ComponentExecutionConfig::default()
        },
        mempool: ComponentExecutionConfig { execute: false, ..ComponentExecutionConfig::default() },
    };

    check_validation_error(
        component_config.validate(),
        "Invalid components configuration.",
        "At least one component should be allowed to execute.",
    );
}

/// Test the validation of the struct ComponentConfig.
/// The validation validates at least one of the components is set with execute: true.
#[rstest]
#[case(true, false, false, false, false)]
#[case(false, true, false, false, false)]
#[case(false, false, true, false, false)]
#[case(false, false, false, true, false)]
#[case(false, false, false, false, true)]
fn test_valid_components_config(
    #[case] batcher_component_execute: bool,
    #[case] consensus_manager_component_execute: bool,
    #[case] gateway_component_execute: bool,
    #[case] http_server_component_execute: bool,
    #[case] mempool_component_execute: bool,
) {
    // Initialize an invalid config and check that the validator finds an error.
    let component_config = ComponentConfig {
        batcher: ComponentExecutionConfig {
            execute: batcher_component_execute,
            ..ComponentExecutionConfig::default()
        },
        consensus_manager: ComponentExecutionConfig {
            execute: consensus_manager_component_execute,
            ..ComponentExecutionConfig::default()
        },
        gateway: ComponentExecutionConfig {
            execute: gateway_component_execute,
            ..ComponentExecutionConfig::default()
        },
        http_server: ComponentExecutionConfig {
            execute: http_server_component_execute,
            ..ComponentExecutionConfig::default()
        },
        mempool: ComponentExecutionConfig {
            execute: mempool_component_execute,
            ..ComponentExecutionConfig::default()
        },
    };

    assert_matches!(component_config.validate(), Ok(()));
}

/// Test the validation of the struct SequencerNodeConfig and that the default config file is up to
/// date. To update the default config file, run:
/// cargo run --bin sequencer_dump_config -q
#[test]
fn default_config_file_is_up_to_date() {
    env::set_current_dir(get_absolute_path("")).expect("Couldn't set working dir.");
    let from_default_config_file: serde_json::Value =
        serde_json::from_reader(File::open(DEFAULT_CONFIG_PATH).unwrap()).unwrap();

    let default_config = SequencerNodeConfig::default();
    assert_matches!(default_config.validate(), Ok(()));

    // Create a temporary file and dump the default config to it.
    let mut tmp_file_path = env::temp_dir();
    tmp_file_path.push("cfg.json");
    default_config.dump_to_file(&CONFIG_POINTERS, tmp_file_path.to_str().unwrap()).unwrap();

    // Read the dumped config from the file.
    let from_code: serde_json::Value =
        serde_json::from_reader(File::open(tmp_file_path).unwrap()).unwrap();

    println!(
        "{}",
        "Default config file doesn't match the default NodeConfig implementation. Please update \
         it using the sequencer_dump_config binary."
            .purple()
            .bold()
    );
    println!(
        "Diffs shown below (default config file <<>> dump of SequencerNodeConfig::default())."
    );
    assert_json_eq!(from_default_config_file, from_code)
}
