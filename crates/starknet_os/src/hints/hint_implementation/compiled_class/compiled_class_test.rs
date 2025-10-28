use std::collections::HashMap;

use blockifier::execution::casm_hash_estimation::{
    CasmV1HashResourceEstimate,
    CasmV2HashResourceEstimate,
    EstimateCasmHashResources,
};
use blockifier::execution::contract_class::{EntryPointV1, EntryPointsByType, NestedFeltCounts};
use blockifier::test_utils::contracts::FeatureContractTrait;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use cairo_lang_starknet_classes::casm_contract_class::{
    CasmContractClass,
    CasmContractEntryPoint,
    CasmContractEntryPoints,
};
use cairo_lang_starknet_classes::NestedIntList;
use cairo_lang_utils::bigint::BigUintAsHex;
use cairo_vm::any_box;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use expect_test::{expect, Expect};
use log::info;
use num_bigint::BigUint;
use rstest::rstest;
use starknet_api::contract_class::compiled_class_hash::{
    HashVersion,
    HashableCompiledClass,
    COMPILED_CLASS_V1,
};
use starknet_api::contract_class::ContractClass;
use starknet_types_core::felt::Felt;

use crate::hints::hint_implementation::compiled_class::utils::create_bytecode_segment_structure;
use crate::hints::vars::{CairoStruct, Const};
use crate::test_utils::cairo_runner::{
    initialize_cairo_runner,
    run_cairo_0_entrypoint,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    ValueArg,
};
use crate::test_utils::utils::DEFAULT_PRIME;
use crate::vm_utils::{get_address_of_nested_fields_from_base_address, LoadCairoObject};

// V1 (Poseidon) HASH CONSTS
/// Expected Poseidon hash for the test contract.
const EXPECTED_V1_HASH: expect_test::Expect =
    expect!["1157029714422828969510047191872039648898471579517702575855872001769107009237"];
// Expected execution resources for loading full contract.
const EXPECTED_BUILTIN_USAGE_FULL_CONTRACT_V1_HASH: expect_test::Expect =
    expect!["poseidon_builtin: 11224"];
const EXPECTED_N_STEPS_FULL_CONTRACT_V1_HASH: Expect = expect!["134274"];
// Expected execution resources for loading partial contract.
const EXPECTED_BUILTIN_USAGE_PARTIAL_CONTRACT_V1_HASH: expect_test::Expect =
    expect!["poseidon_builtin: 350, range_check_builtin: 174"];
const EXPECTED_N_STEPS_PARTIAL_CONTRACT_V1_HASH: Expect = expect!["10469"];
// Allowed margin between estimated and actual execution resources.
const ALLOWED_MARGIN_N_STEPS: usize = 87;

//  V2 (Blake) HASH CONSTS
/// Expected Blake hash for the test contract
const EXPECTED_V2_HASH: expect_test::Expect =
    expect!["1136498274368501956247557789562970166798281498174788902401163669885946661725"];
// Expected execution resources for loading full contract.
const EXPECTED_BUILTIN_USAGE_FULL_CONTRACT_V2_HASH: expect_test::Expect =
    expect!["range_check_builtin: 22856"];
const EXPECTED_N_STEPS_FULL_CONTRACT_V2_HASH: Expect = expect!["436616"];
// Expected execution resources for loading partial contract.
const EXPECTED_BUILTIN_USAGE_PARTIAL_CONTRACT_V2_HASH: expect_test::Expect =
    expect!["range_check_builtin: 992"];
const EXPECTED_N_STEPS_PARTIAL_CONTRACT_V2_HASH: Expect = expect!["42265"];
// Allowed margin between estimated and actual execution resources.
const ALLOWED_MARGIN_BLAKE_N_STEPS: usize = 267;

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
    /// Estimates the execution resources for the compiled class hash function.
    fn estimate_execution_resources(
        &self,
        bytecode_segment_felt_sizes: &NestedFeltCounts,
        entry_points_by_type: &EntryPointsByType<EntryPointV1>,
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
    fn estimate_execution_resources(
        &self,
        bytecode_segment_felt_sizes: &NestedFeltCounts,
        entry_points_by_type: &EntryPointsByType<EntryPointV1>,
    ) -> ExecutionResources {
        match self {
            HashVersion::V1 => {
                CasmV1HashResourceEstimate::estimated_resources_of_compiled_class_hash(
                    bytecode_segment_felt_sizes,
                    entry_points_by_type,
                )
                .resources()
            }
            HashVersion::V2 => {
                CasmV2HashResourceEstimate::estimated_resources_of_compiled_class_hash(
                    bytecode_segment_felt_sizes,
                    entry_points_by_type,
                )
                .resources()
            }
        }
    }
}

fn get_dummy_compiled_class(contract_segmentation: bool) -> CasmContractClass {
    CasmContractClass {
        prime: DEFAULT_PRIME.clone().to_biguint().unwrap(),
        compiler_version: "".into(),
        bytecode: (1u8..=10).map(BigUintAsHex::from).collect(),
        bytecode_segment_lengths: Some(if contract_segmentation {
            NestedIntList::Node(vec![
                NestedIntList::Leaf(3),
                NestedIntList::Node(vec![
                    NestedIntList::Leaf(1),
                    NestedIntList::Leaf(1),
                    NestedIntList::Node(vec![NestedIntList::Leaf(1)]),
                ]),
                NestedIntList::Leaf(4),
            ])
        } else {
            NestedIntList::Leaf(10)
        }),
        entry_points_by_type: CasmContractEntryPoints {
            external: vec![CasmContractEntryPoint {
                selector: BigUint::from(1u8),
                offset: 1,
                builtins: vec!["237".into()],
            }],
            constructor: vec![CasmContractEntryPoint {
                selector: BigUint::from(5u8),
                offset: 0,
                builtins: vec![],
            }],
            l1_handler: vec![],
        },
        hints: vec![],
        pythonic_hints: None,
    }
}

/// Runs the compiled class hash entry point for the given contract class,
/// with the specified load_full_contract flag and hash version.
/// Returns the execution resources and the computed hash.
fn run_compiled_class_hash_entry_point(
    contract_class: &CasmContractClass,
    load_full_contract: bool,
    mark_contract_segments_as_accessed: bool,
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

    // Mark the bytecode segment as accessed if requested.
    if mark_contract_segments_as_accessed {
        // Find the bytecode segment base address.
        let bytecode_ptr_address = get_address_of_nested_fields_from_base_address(
            contract_class_base,
            CairoStruct::CompiledClass,
            &runner.vm,
            &["bytecode_ptr"],
            &program,
        )
        .unwrap();
        let bytecode_ptr = runner.vm.get_relocatable(bytecode_ptr_address).unwrap();
        // Mark as accessed.
        // We cannot use `mark_address_range_as_accessed` because this method cannot be called
        // before the run is finished.
        for i in 0..contract_class.bytecode.len() {
            runner.vm.segments.memory.mark_as_accessed((bytecode_ptr + i).unwrap());
        }
    }

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
#[case::no_segmentation(
    false,
    "0xB268995DD0EE80DEBFB8718852750B5FD22082D0C729121C48A0487A4D2F64",
    16
)]
#[case::segmentation(true, "0x5517AD8471C9AA4D1ADD31837240DEAD9DC6653854169E489A813DB4376BE9C", 28)]
fn test_compiled_class_hash_basic(
    #[case] segmentation: bool,
    #[case] expected_hash: &str,
    #[case] expected_n_poseidons: usize,
) {
    let load_full_contract = false;
    let mark_contract_segments_as_accessed = true;

    let (resources, compiled_class_hash) = run_compiled_class_hash_entry_point(
        &get_dummy_compiled_class(segmentation),
        load_full_contract,
        mark_contract_segments_as_accessed,
        &HashVersion::V1,
    );
    assert_eq!(compiled_class_hash, Felt::from_hex_unchecked(expected_hash));
    assert_eq!(
        *resources.builtin_instance_counter.get(&BuiltinName::poseidon).unwrap(),
        expected_n_poseidons
    );
}

#[rstest]
fn test_compiled_class_hash(
    #[values(true, false)] load_full_contract: bool,
    #[values(true, false)] mark_contract_segments_as_accessed: bool,
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
    let (actual_execution_resources, hash_computed_by_cairo) = run_compiled_class_hash_entry_point(
        &contract_class,
        load_full_contract,
        mark_contract_segments_as_accessed,
        &hash_version,
    );

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
    let (expected_builtin_usage, expected_n_steps) =
        if load_full_contract || mark_contract_segments_as_accessed {
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

/// Test that the estimated execution resources for the compiled class hash
/// match the actual execution resources when running the entry point.
///
/// - `hash_version`: which hash version to test (`V1` (Poseidon) or `V2` (Blake)).
#[rstest]
fn test_compiled_class_hash_resources_estimation(
    #[values(HashVersion::V1, HashVersion::V2)] hash_version: HashVersion,
) {
    for feature_contract in FeatureContract::all_cairo1_casm_feature_contracts() {
        let contract_class = match feature_contract.get_class() {
            ContractClass::V1((casm, _sierra_version)) => casm,
            _ => panic!("Expected ContractClass::V1"),
        };
        let contract_name = feature_contract.get_non_erc20_base_name();
        let bytecode_structure = &contract_class.bytecode_segment_lengths;

        match feature_contract {
            // Legacy test contract is a single leaf segment, this is crucial for testing old sierra
            // contracts.
            FeatureContract::LegacyTestContract => {
                assert!(
                    bytecode_structure.is_none(),
                    "{contract_name}: Expected single segment bytecode."
                );
            }
            // Empty contract is a single leaf segment as its bytecode is empty (no segmentation).
            FeatureContract::Empty(_) => {
                assert!(
                    matches!(bytecode_structure, Some(NestedIntList::Leaf(_))),
                    "{contract_name}: Expected single segment bytecode."
                );
            }
            // Other contracts are node-segmented by their functions.
            _ => {
                assert!(
                    matches!(bytecode_structure, Some(NestedIntList::Node(_))),
                    "{contract_name}: Expected node-segmented bytecode."
                );
            }
        }

        compare_estimated_vs_actual_casm_hash_resources(
            contract_name,
            contract_class,
            &hash_version,
        );
    }
}

fn compare_estimated_vs_actual_casm_hash_resources(
    contract_name: &str,
    contract_class: CasmContractClass,
    hash_version: &HashVersion,
) {
    // Run the compiled class hash entry point with full contract loading.
    let load_full_contract = true;
    let mark_contract_segments_as_accessed = false;
    let (actual_execution_resources, _) = run_compiled_class_hash_entry_point(
        &contract_class,
        load_full_contract,
        mark_contract_segments_as_accessed,
        hash_version,
    );

    let bytecode_segments = NestedFeltCounts::new(
        &contract_class.get_bytecode_segment_lengths(),
        &contract_class.bytecode,
    );

    // Estimate resources.
    let execution_resources_estimation = hash_version.estimate_execution_resources(
        &bytecode_segments,
        &contract_class.entry_points_by_type.into(),
    );

    // Compare n_steps.
    let n_steps_margin =
        execution_resources_estimation.n_steps.abs_diff(actual_execution_resources.n_steps);
    let allowed_n_steps_margin = hash_version.allowed_margin_n_steps();
    assert!(
        n_steps_margin <= allowed_n_steps_margin,
        "{contract_name}: Estimated n_steps differ from actual by more than \
         {allowed_n_steps_margin}. Margin: {n_steps_margin}"
    );

    // Compare builtins.
    assert_eq!(
        execution_resources_estimation.builtin_instance_counter,
        actual_execution_resources.filter_unused_builtins().builtin_instance_counter,
        "{contract_name}: Estimated builtins do not match actual builtins"
    );
}
