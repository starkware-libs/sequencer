use std::collections::HashMap;

use blockifier::execution::contract_class::{
    estimate_casm_blake_hash_computation_resources,
    estimate_casm_poseidon_hash_computation_resources,
    NestedFeltCounts,
};
use blockifier::test_utils::contracts::FeatureContractTrait;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_vm::any_box;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use expect_test::{expect, Expect};
use log::info;
use rstest::rstest;
use starknet_api::contract_class::compiled_class_hash::{
    HashVersion,
    HashableCompiledClass,
    COMPILED_CLASS_V1,
};
use starknet_api::contract_class::ContractClass;
use starknet_types_core::felt::Felt;

use crate::hints::hint_implementation::compiled_class::utils::create_bytecode_segment_structure;
use crate::hints::vars::Const;
use crate::test_utils::cairo_runner::{
    initialize_cairo_runner,
    run_cairo_0_entrypoint,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    ValueArg,
};
use crate::vm_utils::LoadCairoObject;

// V1 (Poseidon) HASH CONSTS
/// Expected Poseidon hash for the test contract.
const EXPECTED_V1_HASH: expect_test::Expect =
    expect!["2699987117682355879179743666679201177869698343279118564476128749435926450101"];
// Expected execution resources for loading full contract.
const EXPECTED_BUILTIN_USAGE_FULL_CONTRACT_V1_HASH: expect_test::Expect =
    expect!["poseidon_builtin: 10290"];
const EXPECTED_N_STEPS_FULL_CONTRACT_V1_HASH: Expect = expect!["122264"];
// Expected execution resources for loading partial contract.
const EXPECTED_BUILTIN_USAGE_PARTIAL_CONTRACT_V1_HASH: expect_test::Expect =
    expect!["poseidon_builtin: 300, range_check_builtin: 149"];
const EXPECTED_N_STEPS_PARTIAL_CONTRACT_V1_HASH: Expect = expect!["8951"];
// Allowed margin between estimated and actual execution resources.
const ALLOWED_MARGIN_N_STEPS: usize = 169;
const ALLOWED_MARGIN_POSEIDON_BUILTIN_V1_HASH: usize = 4;

//  V2 (Blake) HASH CONSTS
/// Expected Blake hash for the test contract
const EXPECTED_V2_HASH: expect_test::Expect =
    expect!["3499480084815927693908408684580831280562065282255124131874976614069883272082"];
// Expected execution resources for loading full contract.
const EXPECTED_BUILTIN_USAGE_FULL_CONTRACT_V2_HASH: expect_test::Expect =
    expect!["range_check_builtin: 20917"];
const EXPECTED_N_STEPS_FULL_CONTRACT_V2_HASH: Expect = expect!["399656"];
// Expected execution resources for loading partial contract.
const EXPECTED_BUILTIN_USAGE_PARTIAL_CONTRACT_V2_HASH: expect_test::Expect =
    expect!["range_check_builtin: 846"];
const EXPECTED_N_STEPS_PARTIAL_CONTRACT_V2_HASH: Expect = expect!["35968"];
// Allowed margin between estimated and actual execution resources.
// TODO(AvivG): lower margins once blake estimation is completed.
const ALLOWED_MARGIN_BLAKE_N_STEPS: usize = 5585;
const ALLOWED_MARGIN_RANGE_CHECK_BUILTIN_V2_HASH: usize = 8;

/// Specifies the expected inputs and outputs for testing a class hash version.
/// Includes entrypoint, bytecode, and expected runtime behavior.
/// Used to validate compiled class hash logic.
trait HashVersionTestSpec {
    fn compiled_class_hash_entrypoint_name(&self) -> &'static str;
    /// The implicit args for the compiled class hash entrypoint.
    fn implicit_args(&self) -> Vec<ImplicitArg>;
    /// The expected builtin usage for the compiled class hash function,
    /// depending on whether the full contract is loaded or not.
    fn expected_builtin_usage_full_contract(&self) -> Expect;
    fn expected_builtin_usage_partial_contract(&self) -> Expect;
    /// The expected number of steps for the compiled class hash function,
    /// depending on whether the full contract is loaded or not.
    fn expected_n_steps_full_contract(&self) -> Expect;
    fn expected_n_steps_partial_contract(&self) -> Expect;
    /// The expected hash for the test contract.
    fn expected_hash(&self) -> Expect;
    /// The allowed margin for the number of steps.
    fn allowed_margin_n_steps(&self) -> usize;
    /// The allowed margin for the builtin usage.
    fn allowed_margin_builtin(&self) -> (BuiltinName, usize);
    /// Estimates the execution resources for the compiled class hash function.
    fn estimate_execution_resources(
        &self,
        bytecode_segment_lengths: &NestedFeltCounts,
    ) -> ExecutionResources;
}

impl HashVersionTestSpec for HashVersion {
    fn compiled_class_hash_entrypoint_name(&self) -> &'static str {
        match self {
            HashVersion::V1 => {
                "starkware.starknet.core.os.contract_class.poseidon_compiled_class_hash.\
                 compiled_class_hash"
            }
            HashVersion::V2 => {
                "starkware.starknet.core.os.contract_class.blake_compiled_class_hash.\
                 compiled_class_hash"
            }
        }
    }
    fn implicit_args(&self) -> Vec<ImplicitArg> {
        match self {
            HashVersion::V1 => vec![
                ImplicitArg::Builtin(BuiltinName::range_check),
                ImplicitArg::Builtin(BuiltinName::poseidon),
            ],
            HashVersion::V2 => vec![ImplicitArg::Builtin(BuiltinName::range_check)],
        }
    }
    fn expected_builtin_usage_full_contract(&self) -> Expect {
        match self {
            HashVersion::V1 => EXPECTED_BUILTIN_USAGE_FULL_CONTRACT_V1_HASH,
            HashVersion::V2 => EXPECTED_BUILTIN_USAGE_FULL_CONTRACT_V2_HASH,
        }
    }

    fn expected_builtin_usage_partial_contract(&self) -> Expect {
        match self {
            HashVersion::V1 => EXPECTED_BUILTIN_USAGE_PARTIAL_CONTRACT_V1_HASH,
            HashVersion::V2 => EXPECTED_BUILTIN_USAGE_PARTIAL_CONTRACT_V2_HASH,
        }
    }

    fn expected_n_steps_full_contract(&self) -> Expect {
        match self {
            HashVersion::V1 => EXPECTED_N_STEPS_FULL_CONTRACT_V1_HASH,
            HashVersion::V2 => EXPECTED_N_STEPS_FULL_CONTRACT_V2_HASH,
        }
    }
    fn expected_n_steps_partial_contract(&self) -> Expect {
        match self {
            HashVersion::V1 => EXPECTED_N_STEPS_PARTIAL_CONTRACT_V1_HASH,
            HashVersion::V2 => EXPECTED_N_STEPS_PARTIAL_CONTRACT_V2_HASH,
        }
    }
    fn expected_hash(&self) -> Expect {
        match self {
            HashVersion::V1 => EXPECTED_V1_HASH,
            HashVersion::V2 => EXPECTED_V2_HASH,
        }
    }
    fn allowed_margin_n_steps(&self) -> usize {
        match self {
            HashVersion::V1 => ALLOWED_MARGIN_N_STEPS,
            HashVersion::V2 => ALLOWED_MARGIN_BLAKE_N_STEPS,
        }
    }
    fn allowed_margin_builtin(&self) -> (BuiltinName, usize) {
        match self {
            HashVersion::V1 => (BuiltinName::poseidon, ALLOWED_MARGIN_POSEIDON_BUILTIN_V1_HASH),
            HashVersion::V2 => {
                (BuiltinName::range_check, ALLOWED_MARGIN_RANGE_CHECK_BUILTIN_V2_HASH)
            }
        }
    }
    fn estimate_execution_resources(
        &self,
        bytecode_segment_lengths: &NestedFeltCounts,
    ) -> ExecutionResources {
        match self {
            HashVersion::V1 => {
                estimate_casm_poseidon_hash_computation_resources(bytecode_segment_lengths)
            }
            HashVersion::V2 => {
                estimate_casm_blake_hash_computation_resources(bytecode_segment_lengths)
                    .resources()
                    .clone()
            }
        }
    }
}

/// Runs the compiled class hash entry point for the given contract class,
/// with the specified load_full_contract flag and hash version.
/// Returns the execution resources and the computed hash.
fn run_compiled_class_hash_entry_point(
    contract_class: &CasmContractClass,
    load_full_contract: bool,
    hash_version: &HashVersion,
) -> (ExecutionResources, Felt) {
    // Set up the entry point runner configuration.
    let runner_config = EntryPointRunnerConfig {
        layout: LayoutName::all_cairo,
        trace_enabled: false,
        verify_secure: false,
        proof_mode: false,
        add_main_prefix_to_entrypoint: false, // Set to false since we're using full path.
    };

    // Set up implicit arguments.
    let implicit_args = hash_version.implicit_args();
    // Expected return value (the hash as a felt).
    let expected_return_values = vec![EndpointArg::from(
        Felt::from_dec_str(hash_version.expected_hash().data())
            .expect("Failed to parse expected hash"),
    )];

    // Set up hint locals for the Cairo runner.
    // This creates a bytecode segment structure from the contract's bytecode and stores it
    // in the hint locals map for use during Cairo program execution.
    let mut hint_locals: HashMap<String, Box<dyn std::any::Any>> = HashMap::new();
    let bytecode_structure = create_bytecode_segment_structure(
        &contract_class.bytecode.iter().map(|x| Felt::from(&x.value)).collect::<Vec<_>>(),
        contract_class.get_bytecode_segment_lengths(),
    )
    .unwrap();
    hint_locals.insert("bytecode_segment_structure".to_string(), any_box!(bytecode_structure));
    let (mut runner, program, entrypoint) = initialize_cairo_runner(
        &runner_config,
        apollo_starknet_os_program::OS_PROGRAM_BYTES,
        hash_version.compiled_class_hash_entrypoint_name(),
        &implicit_args,
        hint_locals,
    )
    .unwrap();
    // Create constants.
    let constants = HashMap::from([(
        <&'static str>::from(Const::CompiledClassVersion).to_string(),
        *COMPILED_CLASS_V1,
    )]);

    // Create explicit arguments for the Cairo entrypoint function.
    // Pass the contract class base address as the function's input parameter.
    let contract_class_base = runner.vm.add_memory_segment();
    contract_class.load_into(&mut runner.vm, &program, contract_class_base, &constants).unwrap();
    let explicit_args = vec![
        // Compiled class
        EndpointArg::Value(ValueArg::Single(contract_class_base.into())),
        // Full contract
        Felt::from(load_full_contract).into(),
    ];
    // Run the Cairo entrypoint function.
    // State reader is not used in this test.
    let state_reader = None;
    let (_implicit_return_values, explicit_return_values) = run_cairo_0_entrypoint(
        entrypoint,
        &explicit_args,
        &implicit_args,
        state_reader,
        &mut runner,
        &program,
        &runner_config,
        &expected_return_values,
    )
    .unwrap();

    // Get the actual execution resources, and compare with expected values.
    let actual_execution_resources = runner.get_execution_resources().unwrap();

    // Get the hash result from the explicit return values.
    let EndpointArg::Value(ValueArg::Single(MaybeRelocatable::Int(hash_computed_by_cairo))) =
        explicit_return_values[0]
    else {
        panic!("Expected a single felt return value");
    };

    (actual_execution_resources, hash_computed_by_cairo)
}

#[rstest]
fn test_compiled_class_hash(
    #[values(true, false)] load_full_contract: bool,
    #[values(HashVersion::V1, HashVersion::V2)] hash_version: HashVersion,
) {
    // Get the test contract class.
    let feature_contract =
        FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let contract_class = match feature_contract.get_class() {
        ContractClass::V1((casm, _sierra_version)) => casm,
        _ => panic!("Expected ContractClass::V1"),
    };
    // Run the compiled class hash entry point.
    let (actual_execution_resources, hash_computed_by_cairo) =
        run_compiled_class_hash_entry_point(&contract_class, load_full_contract, &hash_version);

    // Format builtin usage statistics for comparison with expected values.
    // Filter out unused builtins (count = 0), format as "name: count", sort alphabetically,
    // and join with commas for consistent test output.
    let mut actual_builtin_usage_parts: Vec<_> = actual_execution_resources
        .builtin_instance_counter
        .iter()
        .filter(|(_, &count)| count > 0)
        .map(|(name, count)| format!("{}: {}", name.to_str_with_suffix(), count))
        .collect();
    actual_builtin_usage_parts.sort();
    let actual_builtin_usage = actual_builtin_usage_parts.join(", ");

    // Select expected values based on whether we're loading full or partial contract.
    let (expected_builtin_usage, expected_n_steps) = if load_full_contract {
        (
            hash_version.expected_builtin_usage_full_contract(),
            hash_version.expected_n_steps_full_contract(),
        )
    } else {
        (
            hash_version.expected_builtin_usage_partial_contract(),
            hash_version.expected_n_steps_partial_contract(),
        )
    };

    expected_builtin_usage.assert_eq(&actual_builtin_usage);
    expected_n_steps.assert_eq(&actual_execution_resources.n_steps.to_string());

    info!("Computed compiled class hash: {hash_computed_by_cairo}");
    // Verify the hash is not zero (a basic sanity check).
    // Use expect! macro for easy test maintenance.
    hash_version.expected_hash().assert_eq(&hash_computed_by_cairo.to_string());

    // Compare with the hash computed by the starknet_api.
    let hash_computed_by_starknet_api = contract_class.hash(&hash_version);
    assert_eq!(hash_computed_by_cairo, hash_computed_by_starknet_api.0);
}

/// Test that execution resources estimation for the compiled class hash
/// matches the actual execution resources when running the entry point.
#[rstest]
fn test_compiled_class_hash_resources_estimation(
    #[values(HashVersion::V1, HashVersion::V2)] hash_version: HashVersion,
) {
    let feature_contract =
        FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let mut contract_class = match feature_contract.get_class() {
        ContractClass::V1((casm, _sierra_version)) => casm,
        _ => panic!("Expected ContractClass::V1"),
    };

    // TODO(Aviv): Remove this once we estimate correctly compiled class hash with entry-points.
    contract_class.entry_points_by_type = Default::default();

    // Run the compiled class hash entry point with full contract loading.
    let (mut actual_execution_resources, _hash_computed_by_cairo) =
        run_compiled_class_hash_entry_point(&contract_class, true, &hash_version);

    // Compare the actual execution resources with the estimation with some allowed margin.
    let mut execution_resources_estimation =
        hash_version.estimate_execution_resources(&NestedFeltCounts::new(
            &contract_class.get_bytecode_segment_lengths(),
            &contract_class.bytecode,
        ));
    let margin_n_steps =
        execution_resources_estimation.n_steps.abs_diff(actual_execution_resources.n_steps);
    let allowed_margin = hash_version.allowed_margin_n_steps();
    assert!(
        margin_n_steps <= allowed_margin,
        "Estimated n_steps and actual n_steps differ by more than {allowed_margin}.\n Margin N \
         Steps: {margin_n_steps}"
    );
    let (builtin_name, allowed_margin_builtin) = hash_version.allowed_margin_builtin();
    let margin_builtin = execution_resources_estimation
        .builtin_instance_counter
        .remove(&builtin_name)
        .unwrap()
        .abs_diff(
            actual_execution_resources.builtin_instance_counter.remove(&builtin_name).unwrap(),
        );
    assert!(
        margin_builtin <= allowed_margin_builtin,
        "Estimated {builtin_name} and actual {builtin_name} differ by more than \
         {allowed_margin_builtin}.\n Margin {builtin_name} Builtin: {margin_builtin}"
    );

    // Assert that all other builtins have exactly the same values.
    assert_eq!(
        execution_resources_estimation.builtin_instance_counter,
        actual_execution_resources.filter_unused_builtins().builtin_instance_counter
    );
}
