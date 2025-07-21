use std::collections::HashMap;

use blockifier::test_utils::contracts::FeatureContractTrait;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use cairo_vm::any_box;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use expect_test::expect;
use log::info;
use starknet_api::contract_class::ContractClass;
use starknet_types_core::felt::Felt;

use crate::hints::hint_implementation::compiled_class::implementation::COMPILED_CLASS_V1;
use crate::hints::hint_implementation::compiled_class::utils::create_bytecode_segment_structure;
use crate::hints::vars::{Const, Scope};
use crate::test_utils::cairo_runner::{
    initialize_cairo_runner,
    run_cairo_0_entrypoint,
    Cairo0EntryPointRunnerResult,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    ValueArg,
};
use crate::vm_utils::LoadCairoObject;

/// Expected Poseidon hash for the test contract.
const EXPECTED_HASH: expect_test::Expect =
    expect!["2150674463996749144683780989243380489636302685101988784859767903560019656687"];

// TODO(Aviv): Share this test with compiled class hash blake test.
#[test]
fn test_compiled_class_hash_poseidon() -> Cairo0EntryPointRunnerResult<()> {
    // Set up the entry point runner configuration.
    let runner_config = EntryPointRunnerConfig {
        layout: LayoutName::all_cairo,
        trace_enabled: false,
        verify_secure: false,
        proof_mode: false,
        add_main_prefix_to_entrypoint: false, // Set to false since we're using full path.
    };

    // Set up implicit arguments.
    let implicit_args = vec![
        ImplicitArg::Builtin(BuiltinName::range_check),
        ImplicitArg::Builtin(BuiltinName::poseidon),
    ];
    // Expected return value (the hash as a felt).
    let expected_return_values = vec![EndpointArg::from(
        Felt::from_dec_str(EXPECTED_HASH.data()).expect("Failed to parse EXPECTED_HASH"),
    )];
    // Get the OS program as bytes.
    let program_bytes = apollo_starknet_os_program::OS_PROGRAM_BYTES;
    // Get the test contract class.
    let feature_contract =
        FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let contract_class = match feature_contract.get_class() {
        ContractClass::V1((casm, _sierra_version)) => casm,
        _ => panic!("Expected ContractClass::V1"),
    };

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
    // Set leaf_always_accessed to true in the root level exec scope.
    hint_locals.insert(<&'static str>::from(Scope::LeafAlwaysAccessed).to_string(), any_box!(true));
    // Use the Poseidon version.
    let (mut runner, program, entrypoint) = initialize_cairo_runner(
        &runner_config,
        program_bytes,
        "starkware.starknet.core.os.contract_class.poseidon_compiled_class_hash.\
         compiled_class_hash",
        &implicit_args,
        hint_locals,
    )?;
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
        // TODO(Meshi): Test with full contract true as well.
        Felt::from(0).into(),
    ];
    // Run the Cairo entrypoint function.
    // State reader is not used in this test.
    let state_reader = None;
    // Validations are not supported since we loaded the contract class by ourselves.
    let skip_parameter_validations = true;
    let (implicit_return_values, explicit_return_values) = run_cairo_0_entrypoint(
        entrypoint,
        &explicit_args,
        &implicit_args,
        state_reader,
        &mut runner,
        &program,
        &runner_config,
        &expected_return_values,
        skip_parameter_validations,
    )
    .unwrap();

    // Verify we got a return value.
    assert_eq!(explicit_return_values.len(), 1, "Expected exactly one return value");
    assert!(!implicit_return_values.is_empty(), "Expected implicit return values");
    // The return value should be a felt (the computed hash).
    let EndpointArg::Value(ValueArg::Single(MaybeRelocatable::Int(hash_computed_by_the_os))) =
        &explicit_return_values[0]
    else {
        panic!("Expected a single felt return value");
    };

    info!("Computed Poseidon compiled class hash: {hash_computed_by_the_os}");
    // Verify the hash is not zero (a basic sanity check).
    // Use expect! macro for easy test maintenance.
    EXPECTED_HASH.assert_eq(&hash_computed_by_the_os.to_string());

    // Compare with the hash computed by the compiler.
    let hash_computed_by_compiler = contract_class.compiled_class_hash();
    assert_eq!(*hash_computed_by_the_os, hash_computed_by_compiler);
    Ok(())
}
