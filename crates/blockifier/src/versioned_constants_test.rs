use glob::{glob, Paths};
use pretty_assertions::assert_eq;

use super::*;

// TODO: Test Starknet OS validation.
// TODO: Add an unallowed field scenario for GasCost parsing.

/// Returns all JSON files in the resources directory (should be all versioned constants files).
fn all_jsons_in_dir() -> Paths {
    glob(format!("{}/resources/*.json", compile_time_cargo_manifest_dir!()).as_str()).unwrap()
}

#[test]
fn test_successful_gas_costs_parsing() {
    let json_data = r#"
    {
        "step_gas_cost": 2,
        "entry_point_initial_budget": {
            "step_gas_cost": 3
        },
        "syscall_base_gas_cost": {
            "entry_point_initial_budget": 4,
            "step_gas_cost": 5
        },
        "error_out_of_gas": "An additional field in GasCosts::ADDITIONAL_ALLOWED_NAMES, ignored."
    }"#;
    let gas_costs = GasCosts::create_for_testing_from_subset(json_data);
    let os_constants: Arc<OsConstants> = Arc::new(OsConstants { gas_costs, ..Default::default() });
    let versioned_constants = VersionedConstants { os_constants, ..Default::default() };

    assert_eq!(versioned_constants.os_constants.gas_costs.base.step_gas_cost, 2);
    assert_eq!(versioned_constants.os_constants.gas_costs.base.entry_point_initial_budget, 2 * 3); // step_gas_cost * 3.

    // entry_point_initial_budget * 4 + step_gas_cost * 5.
    assert_eq!(
        versioned_constants.os_constants.gas_costs.base.syscall_base_gas_cost,
        6 * 4 + 2 * 5
    );
}

/// Assert versioned constants overrides are used when provided.
#[test]
fn test_versioned_constants_overrides() {
    let versioned_constants = VersionedConstants::latest_constants().clone();
    let updated_invoke_tx_max_n_steps = versioned_constants.invoke_tx_max_n_steps + 1;
    let updated_validate_max_n_steps = versioned_constants.validate_max_n_steps + 1;
    let updated_max_recursion_depth = versioned_constants.max_recursion_depth + 1;
    let updated_max_n_events = versioned_constants.tx_event_limits.max_n_emitted_events + 1;

    // Create a versioned constants copy with overriden values.
    let result = VersionedConstants::get_versioned_constants(VersionedConstantsOverrides {
        validate_max_n_steps: updated_validate_max_n_steps,
        max_recursion_depth: updated_max_recursion_depth,
        invoke_tx_max_n_steps: updated_invoke_tx_max_n_steps,
        max_n_events: updated_max_n_events,
    });

    // Assert the new values are used.
    assert_eq!(result.invoke_tx_max_n_steps, updated_invoke_tx_max_n_steps);
    assert_eq!(result.validate_max_n_steps, updated_validate_max_n_steps);
    assert_eq!(result.max_recursion_depth, updated_max_recursion_depth);
    assert_eq!(result.tx_event_limits.max_n_emitted_events, updated_max_n_events);
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
    json_data_raw.insert("validate_block_number_rounding".into(), 0.into());
    json_data_raw.insert("validate_timestamp_rounding".into(), 0.into());
    json_data_raw.insert(
        "os_contract_addresses".into(),
        serde_json::to_value(OsContractAddresses::default()).unwrap(),
    );
    json_data_raw.insert("v1_bound_accounts_cairo0".into(), serde_json::Value::Array(vec![]));
    json_data_raw.insert("v1_bound_accounts_cairo1".into(), serde_json::Value::Array(vec![]));
    json_data_raw.insert("v1_bound_accounts_max_tip".into(), "0x0".into());

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

/// V1 bound accounts can only be added in later versions, not removed.
#[test]
fn test_v1_bound_accounts_increasing() {
    let first_version = StarknetVersion::V0_13_0;
    let mut prev_vc = VersionedConstants::get(&first_version).unwrap();
    let mut prev_version = first_version;
    for version in StarknetVersion::iter().filter(|v| v > &first_version) {
        let current_vc = VersionedConstants::get(&version).unwrap();
        assert!(
            HashSet::<ClassHash>::from_iter(
                current_vc.os_constants.v1_bound_accounts_cairo0.iter().cloned()
            )
            .is_superset(&HashSet::from_iter(
                prev_vc.os_constants.v1_bound_accounts_cairo0.iter().cloned()
            )),
            "v1_bound_accounts_cairo0 is not a superset from version {prev_version} to {version}",
        );
        assert!(
            HashSet::<ClassHash>::from_iter(
                current_vc.os_constants.v1_bound_accounts_cairo1.iter().cloned()
            )
            .is_superset(&HashSet::from_iter(
                prev_vc.os_constants.v1_bound_accounts_cairo1.iter().cloned()
            )),
            "v1_bound_accounts_cairo1 is not a superset from version {prev_version} to {version}",
        );
        // TODO(Dori): when this reaches main-v0.14.0, add a similar check for data_gas_accounts.
        prev_version = version;
        prev_vc = current_vc;
    }
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
    let all_jsons: Vec<PathBuf> = all_jsons_in_dir().map(Result::unwrap).collect();

    // Check that the number of new starknet versions (versions supporting VC) is equal to the
    // number of JSON files.
    assert_eq!(
        StarknetVersion::iter().filter(|version| version >= &StarknetVersion::V0_13_0).count(),
        all_jsons.len()
    );

    // Check that all JSON files are in the enum and can be loaded.
    for file in all_jsons {
        let filename = file.file_stem().unwrap().to_str().unwrap().to_string();
        assert!(filename.starts_with("versioned_constants_"));
        let version_str = filename.trim_start_matches("versioned_constants_").replace("_", ".");
        let version = StarknetVersion::try_from(version_str).unwrap();
        assert!(VersionedConstants::get(&version).is_ok());
    }
}

#[test]
fn test_latest_no_panic() {
    VersionedConstants::latest_constants();
}

#[test]
fn test_syscall_gas_cost_calculation() {
    const EXPECTED_CALL_CONTRACT_GAS_COST: u64 = 87650;
    const EXPECTED_SECP256K1MUL_GAS_COST: u64 = 8143650;
    const EXPECTED_SHA256PROCESSBLOCK_GAS_COST: u64 = 841095;

    let versioned_constants = VersionedConstants::latest_constants().clone();

    assert_eq!(
        versioned_constants.get_syscall_gas_cost(&SyscallSelector::CallContract).base,
        EXPECTED_CALL_CONTRACT_GAS_COST
    );
    assert_eq!(
        versioned_constants.get_syscall_gas_cost(&SyscallSelector::Secp256k1Mul).base,
        EXPECTED_SECP256K1MUL_GAS_COST
    );
    assert_eq!(
        versioned_constants.get_syscall_gas_cost(&SyscallSelector::Sha256ProcessBlock).base,
        EXPECTED_SHA256PROCESSBLOCK_GAS_COST
    );
}

/// Linear gas cost factor of deploy syscall should not be trivial.
#[test]
fn test_call_data_factor_gas_cost_calculation() {
    assert!(
        VersionedConstants::latest_constants().os_constants.gas_costs.syscalls.deploy.linear_factor
            > 0
    )
}
