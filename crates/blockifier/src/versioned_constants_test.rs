use glob::{glob, Paths};
use pretty_assertions::assert_eq;

use super::*;

// TODO: Test Starknet OS validation.
// TODO: Add an unallowed field scenario for GasCost parsing.

/// Returns all JSON files in the resources directory (should be all versioned constants files).
fn all_jsons_in_dir() -> Paths {
    glob(format!("{}/resources/*.json", env!("CARGO_MANIFEST_DIR")).as_str()).unwrap()
}

#[test]
fn test_successful_gas_costs_parsing() {
    let json_data = r#"
    {
        "step_gas_cost": 2,
        "entry_point_initial_budget": {
            "step_gas_cost": 3
        },
        "entry_point_gas_cost": {
            "entry_point_initial_budget": 4,
            "step_gas_cost": 5
        },
        "error_out_of_gas": "An additional field in GasCosts::ADDITIONAL_ALLOWED_NAMES, ignored."
    }"#;
    let gas_costs = GasCosts::create_for_testing_from_subset(json_data);
    let os_constants: Arc<OsConstants> = Arc::new(OsConstants { gas_costs, ..Default::default() });
    let versioned_constants = VersionedConstants { os_constants, ..Default::default() };

    assert_eq!(versioned_constants.os_constants.gas_costs.step_gas_cost, 2);
    assert_eq!(versioned_constants.os_constants.gas_costs.entry_point_initial_budget, 2 * 3); // step_gas_cost * 3.

    // entry_point_initial_budget * 4 + step_gas_cost * 5.
    assert_eq!(versioned_constants.os_constants.gas_costs.entry_point_gas_cost, 6 * 4 + 2 * 5);
}

/// Assert versioned constants overrides are used when provided.
#[test]
fn test_versioned_constants_overrides() {
    let versioned_constants = VersionedConstants::latest_constants().clone();
    let updated_invoke_tx_max_n_steps = versioned_constants.invoke_tx_max_n_steps + 1;
    let updated_validate_max_n_steps = versioned_constants.validate_max_n_steps + 1;
    let updated_max_recursion_depth = versioned_constants.max_recursion_depth + 1;

    // Create a versioned constants copy with overriden values.
    let result = VersionedConstants::get_versioned_constants(VersionedConstantsOverrides {
        validate_max_n_steps: updated_validate_max_n_steps,
        max_recursion_depth: updated_max_recursion_depth,
        invoke_tx_max_n_steps: updated_invoke_tx_max_n_steps,
    });

    // Assert the new values are used.
    assert_eq!(result.invoke_tx_max_n_steps, updated_invoke_tx_max_n_steps);
    assert_eq!(result.validate_max_n_steps, updated_validate_max_n_steps);
    assert_eq!(result.max_recursion_depth, updated_max_recursion_depth);
}

#[test]
fn test_string_inside_composed_field() {
    let json_data = r#"
    {
        "step_gas_cost": 2,
        "entry_point_initial_budget": {
            "step_gas_cost": "meow"
        }
    }"#;

    check_constants_serde_error(
        json_data,
        "Value \"meow\" used to create value for key 'entry_point_initial_budget' is out of range \
         and cannot be cast into u64",
    );
}

fn check_constants_serde_error(json_data: &str, expected_error_message: &str) {
    let mut json_data_raw: IndexMap<String, Value> = serde_json::from_str(json_data).unwrap();
    json_data_raw.insert("validate_block_number_rounding".to_string(), 0.into());
    json_data_raw.insert("validate_timestamp_rounding".to_string(), 0.into());

    let json_data = &serde_json::to_string(&json_data_raw).unwrap();

    let error = serde_json::from_str::<OsConstants>(json_data).unwrap_err();
    assert_eq!(error.to_string(), expected_error_message);
}

#[test]
fn test_missing_key() {
    let json_data = r#"
    {
        "entry_point_initial_budget": {
            "TEN LI GAZ!": 2
        }
    }"#;
    check_constants_serde_error(
        json_data,
        "Unknown key 'TEN LI GAZ!' used to create value for 'entry_point_initial_budget'",
    );
}

#[test]
fn test_unhandled_value_type() {
    let json_data = r#"
    {
        "step_gas_cost": []
    }"#;
    check_constants_serde_error(json_data, "Unhandled value type: []");
}

#[test]
fn test_invalid_number() {
    check_constants_serde_error(
        r#"{"step_gas_cost": 42.5}"#,
        "Value 42.5 for key 'step_gas_cost' is out of range and cannot be cast into u64",
    );

    check_constants_serde_error(
        r#"{"step_gas_cost": -2}"#,
        "Value -2 for key 'step_gas_cost' is out of range and cannot be cast into u64",
    );

    let json_data = r#"
    {
        "step_gas_cost": 2,
        "entry_point_initial_budget": {
            "step_gas_cost": 42.5
        }
    }"#;
    check_constants_serde_error(
        json_data,
        "Value 42.5 used to create value for key 'entry_point_initial_budget' is out of range and \
         cannot be cast into u64",
    );
}

#[test]
fn test_old_json_parsing() {
    for file in all_jsons_in_dir().map(Result::unwrap) {
        serde_json::from_reader::<_, VersionedConstants>(&std::fs::File::open(&file).unwrap())
            .unwrap_or_else(|_| panic!("Versioned constants JSON file {file:#?} is malformed"));
    }
}

#[test]
fn test_all_jsons_in_enum() {
    assert_eq!(StarknetVersion::iter().count(), all_jsons_in_dir().count());
}

#[test]
fn test_latest_no_panic() {
    VersionedConstants::latest_constants();
}
