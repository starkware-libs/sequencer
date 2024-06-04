#[cfg(any(feature = "testing", test))]
use std::env::{self};
use std::fs::File;

use assert_json_diff::assert_json_eq;
use assert_matches::assert_matches;
use colored::Colorize;
use papyrus_config::dumping::SerializeConfig;
use papyrus_config::validators::{ParsedValidationError, ParsedValidationErrors};
use test_utils::get_absolute_path;
use validator::Validate;

use crate::config::{
    ComponentConfig, ComponentExecutionConfig, MempoolNodeConfig, DEFAULT_CONFIG_PATH,
};

/// Test the validation of the struct ComponentConfig.
/// The validation validates at least one of the components is set with execute: true.
#[test]
fn test_components_config_validation() {
    // Initialize an invalid config and check that the validator finds an error.
    let mut component_config = ComponentConfig {
        gateway_component: ComponentExecutionConfig { execute: false },
        mempool_component: ComponentExecutionConfig { execute: false },
    };

    assert_matches!(component_config.validate().unwrap_err(), validation_errors => {
        let parsed_errors = ParsedValidationErrors::from(validation_errors);
        assert_eq!(parsed_errors.0.len(), 1);
        let parsed_validation_error = &parsed_errors.0[0];
        assert_matches!(
            parsed_validation_error,
            ParsedValidationError { param_path, code, message, params}
            if (
                param_path == "__all__" &&
                code == "Invalid components configuration." &&
                params.is_empty() &&
                *message == Some("At least one component should be allowed to execute.".to_string())
            )
        )
    });

    // Update the config to be valid and check that the validator finds no errors.
    for (gateway_component_execute, mempool_component_execute) in
        [(true, false), (false, true), (true, true)]
    {
        component_config.gateway_component.execute = gateway_component_execute;
        component_config.mempool_component.execute = mempool_component_execute;

        assert_matches!(component_config.validate(), Ok(()));
    }
}

/// Test the validation of the struct MempoolNodeConfig and that the default config file is up to
/// date. To update the default config file, run:
/// cargo run --bin dump_config -q
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
         it using the dump_config binary."
            .purple()
            .bold()
    );
    println!("Diffs shown below.");
    assert_json_eq!(from_default_config_file, from_code)
}
