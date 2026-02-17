use blockifier::execution::contract_class::CompiledClassV1;
use blockifier::test_utils::contracts::FeatureContractTrait;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use starknet_api::contract_class::ContractClass;

use crate::running::classes_provider::compiled_class_v1_to_casm;

/// Tests the round-trip conversion: CasmContractClass -> CompiledClassV1 -> CasmContractClass
/// Verifies that all relevant fields are preserved (except compiler_version and pythonic_hints
/// which are not loaded to the OS).
#[test]
fn test_compiled_class_v1_to_casm_round_trip() {
    // Get a test contract that has hints and entry points.
    let feature_contract =
        FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let (mut original_casm, sierra_version) = match feature_contract.get_class() {
        ContractClass::V1(versioned_casm) => versioned_casm,
        _ => panic!("Expected ContractClass::V1"),
    };

    // Ignore fields that are not loaded to the OS.
    original_casm.compiler_version = String::new();
    original_casm.pythonic_hints = None;

    // Convert CasmContractClass to CompiledClassV1.
    let compiled_class_v1 = CompiledClassV1::try_from((original_casm.clone(), sierra_version))
        .expect("Failed to convert CasmContractClass to CompiledClassV1");

    // Convert back to CasmContractClass using compiled_class_v1_to_casm.
    let round_tripped_casm = compiled_class_v1_to_casm(&compiled_class_v1)
        .expect("Failed to convert CompiledClassV1 back to CasmContractClass");

    // Verify that the round-tripped CasmContractClass is equal to the original.
    assert_eq!(round_tripped_casm, original_casm);
}
