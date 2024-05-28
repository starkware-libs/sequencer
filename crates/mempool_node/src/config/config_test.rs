#[cfg(any(feature = "testing", test))]
use assert_matches::assert_matches;
use papyrus_config::validators::{ParsedValidationError, ParsedValidationErrors};
use validator::Validate;

use crate::config::{ComponentConfig, ComponentExecutionConfig};

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

        assert!(component_config.validate().is_ok());
    }
}
