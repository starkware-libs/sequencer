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
    RemoteComponentCommunicationConfig,
};
use validator::{Validate, ValidationErrors};

use crate::config::{
    ComponentConfig,
    ComponentExecutionConfig,
    LocationType,
    MempoolNodeConfig,
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
/// The validation validates that location of the component and the local/remote config are at sync.
#[rstest]
#[case(
    LocationType::Local,
    Some(LocalComponentCommunicationConfig::default()),
    Some(RemoteComponentCommunicationConfig::default()),
    "Local config and Remote config are mutually exclusive, can't be both active."
)]
#[case(
    LocationType::Local,
    None,
    Some(RemoteComponentCommunicationConfig::default()),
    "Local communication config is missing."
)]
#[case(LocationType::Local, None, None, "Local communication config is missing.")]
#[case(
    LocationType::Remote,
    Some(LocalComponentCommunicationConfig::default()),
    Some(RemoteComponentCommunicationConfig::default()),
    "Local config and Remote config are mutually exclusive, can't be both active."
)]
#[case(
    LocationType::Remote,
    Some(LocalComponentCommunicationConfig::default()),
    None,
    "Remote communication config is missing."
)]
#[case(LocationType::Remote, None, None, "Remote communication config is missing.")]
fn test_invalid_component_execution_config(
    #[case] location: LocationType,
    #[case] local_config: Option<LocalComponentCommunicationConfig>,
    #[case] remote_config: Option<RemoteComponentCommunicationConfig>,
    #[case] expected_error_message: &str,
) {
    // Initialize an invalid config and check that the validator finds an error.
    let component_exe_config = ComponentExecutionConfig {
        location,
        local_config,
        remote_config,
        ..ComponentExecutionConfig::default()
    };
    check_validation_error(
        component_exe_config.validate(),
        "Invalid component configuration.",
        expected_error_message,
    );
}

/// Test the validation of the struct ComponentExecutionConfig.
/// The validation validates that location of the component and the local/remote config are at sync.
#[rstest]
#[case::local(LocationType::Local)]
#[case::remote(LocationType::Remote)]
fn test_valid_component_execution_config(#[case] location: LocationType) {
    // Initialize a valid config and check that the validator returns Ok.
    let local_config = if location == LocationType::Local {
        Some(LocalComponentCommunicationConfig::default())
    } else {
        None
    };
    let remote_config = if location == LocationType::Remote {
        Some(RemoteComponentCommunicationConfig::default())
    } else {
        None
    };
    let component_exe_config = ComponentExecutionConfig {
        location,
        local_config,
        remote_config,
        ..ComponentExecutionConfig::default()
    };
    assert!(component_exe_config.validate().is_ok());
}

#[test]
fn test_invalid_components_config() {
    // Initialize an invalid config and check that the validator finds an error.
    let component_config = ComponentConfig {
        batcher: ComponentExecutionConfig { execute: false, ..ComponentExecutionConfig::default() },
        gateway: ComponentExecutionConfig { execute: false, ..ComponentExecutionConfig::default() },
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
#[case(true, false, false)]
#[case(false, true, false)]
#[case(false, false, true)]
fn test_valid_components_config(
    #[case] batcher_component_execute: bool,
    #[case] gateway_component_execute: bool,
    #[case] mempool_component_execute: bool,
) {
    // Initialize an invalid config and check that the validator finds an error.
    let component_config = ComponentConfig {
        batcher: ComponentExecutionConfig {
            execute: batcher_component_execute,
            ..ComponentExecutionConfig::default()
        },
        gateway: ComponentExecutionConfig {
            execute: gateway_component_execute,
            ..ComponentExecutionConfig::default()
        },
        mempool: ComponentExecutionConfig {
            execute: mempool_component_execute,
            ..ComponentExecutionConfig::default()
        },
    };

    assert_matches!(component_config.validate(), Ok(()));
}

/// Test the validation of the struct MempoolNodeConfig and that the default config file is up to
/// date. To update the default config file, run:
/// cargo run --bin mempool_dump_config -q
#[test]
fn default_config_file_is_up_to_date() {
    let default_config = MempoolNodeConfig::default();
    assert_matches!(default_config.validate(), Ok(()));
    let from_code: serde_json::Value = serde_json::to_value(default_config.dump()).unwrap();

    env::set_current_dir(get_absolute_path("")).expect("Couldn't set working dir.");
    let from_default_config_file: serde_json::Value =
        serde_json::from_reader(File::open(DEFAULT_CONFIG_PATH).unwrap()).unwrap();

    println!(
        "{}",
        "Default config file doesn't match the default NodeConfig implementation. Please update \
         it using the mempool_dump_config binary."
            .purple()
            .bold()
    );
    println!("Diffs shown below.");
    assert_json_eq!(from_default_config_file, from_code)
}
