use std::env;
use std::fs::File;

use assert_json_diff::assert_json_eq;
use assert_matches::assert_matches;
use colored::Colorize;
use mempool_test_utils::get_absolute_path;
use papyrus_config::dumping::SerializeConfig;
use papyrus_config::validators::{ParsedValidationError, ParsedValidationErrors};
use starknet_mempool_infra::component_server::{
    LocalComponentServerConfig,
    RemoteComponentServerConfig,
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
#[test]
fn test_component_execution_config_validation() {
    // Initialize an invalid config and check that the validator finds an error.
    let mut component_exe_config = ComponentExecutionConfig {
        location: LocationType::Local,
        local_config: None,
        ..ComponentExecutionConfig::default()
    };
    check_validation_error(
        component_exe_config.validate(),
        "Invalid component configuration.",
        "The component is Local but configuered as remote.",
    );

    // Initialize a valid config and check that the validator returns Ok.
    component_exe_config.local_config = Some(LocalComponentServerConfig::default());
    assert!(component_exe_config.validate().is_ok());

    // Initialize an invalid config and check that the validator finds an error.
    component_exe_config = ComponentExecutionConfig {
        location: LocationType::Remote,
        remote_config: None,
        ..ComponentExecutionConfig::default()
    };
    check_validation_error(
        component_exe_config.validate(),
        "Invalid component configuration.",
        "The component is Remote but configuered as local.",
    );

    // Initialize a valid config and check that the validator returns Ok.
    component_exe_config.remote_config = Some(RemoteComponentServerConfig::default());
    assert!(component_exe_config.validate().is_ok());
}

/// Test the validation of the struct ComponentConfig.
/// The validation validates at least one of the components is set with execute: true.
#[test]
fn test_components_config_validation() {
    // Initialize an invalid config and check that the validator finds an error.
    let mut component_config = ComponentConfig {
        gateway: ComponentExecutionConfig { execute: false, ..ComponentExecutionConfig::default() },
        mempool: ComponentExecutionConfig { execute: false, ..ComponentExecutionConfig::default() },
    };

    check_validation_error(
        component_config.validate(),
        "Invalid components configuration.",
        "At least one component should be allowed to execute.",
    );

    // Update the config to be valid and check that the validator finds no errors.
    for (gateway_component_execute, mempool_component_execute) in
        [(true, false), (false, true), (true, true)]
    {
        component_config.gateway.execute = gateway_component_execute;
        component_config.mempool.execute = mempool_component_execute;

        assert_matches!(component_config.validate(), Ok(()));
    }
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
