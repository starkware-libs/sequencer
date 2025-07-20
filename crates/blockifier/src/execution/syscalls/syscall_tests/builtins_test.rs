use std::sync::Arc;

use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
#[cfg(feature = "cairo_native")]
use cairo_vm::types::builtin_name::BuiltinName;
use rstest::rstest;
use rstest_reuse::apply;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::calldata;

use crate::blockifier_versioned_constants::BuiltinGasCosts;
use crate::context::{BlockContext, ChainInfo};
use crate::execution::entry_point::CallEntryPoint;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::test_templates::runnable_version;
use crate::test_utils::{trivial_external_entry_point_new, BALANCE};

const TESTED_BUILTIN_GAS_COST: u64 = u64::pow(10, 7);

#[apply(runnable_version)]
#[case::pedersen("test_pedersen")]
#[case::bitwise("test_bitwise")]
#[case::ecop("test_ecop")]
#[case::poseidon("test_poseidon")]
// This test case tests the add_mod and mul_mod builtins.
#[case::add_and_mul_mod("test_circuit")]
fn builtins_test(runnable_version: RunnableCairo1, #[case] selector_name: &str) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(test_contract, 1)]);

    let calldata = calldata![];
    let entry_point_call = CallEntryPoint {
        entry_point_selector: selector_from_name(selector_name),
        calldata,
        ..trivial_external_entry_point_new(test_contract)
    };

    let mut block_context = BlockContext::create_for_account_testing();
    assert!(
        block_context.versioned_constants.os_constants.execute_max_sierra_gas.0
            > TESTED_BUILTIN_GAS_COST
    );
    change_builtins_gas_cost(&mut block_context, selector_name);
    let mut minimal_gas = TESTED_BUILTIN_GAS_COST;
    if selector_name == "test_circuit" {
        minimal_gas *= 2;
    }

    let call_info =
        entry_point_call.execute_directly_given_block_context(&mut state, block_context).unwrap();

    assert!(!call_info.execution.failed, "Execution failed");
    assert!(call_info.execution.gas_consumed >= minimal_gas);
}

fn change_builtins_gas_cost(block_context: &mut BlockContext, selector_name: &str) {
    let os_constants = Arc::make_mut(&mut block_context.versioned_constants.os_constants);
    os_constants.gas_costs.builtins = BuiltinGasCosts::default();
    match selector_name {
        "test_pedersen" => {
            os_constants.gas_costs.builtins.pedersen = TESTED_BUILTIN_GAS_COST;
        }
        "test_bitwise" => {
            os_constants.gas_costs.builtins.bitwise = TESTED_BUILTIN_GAS_COST;
        }
        "test_ecop" => {
            os_constants.gas_costs.builtins.ecop = TESTED_BUILTIN_GAS_COST;
        }
        "test_poseidon" => {
            os_constants.gas_costs.builtins.poseidon = TESTED_BUILTIN_GAS_COST;
        }
        "test_circuit" => {
            os_constants.gas_costs.builtins.add_mod = TESTED_BUILTIN_GAS_COST;
            os_constants.gas_costs.builtins.mul_mod = TESTED_BUILTIN_GAS_COST;
        }
        _ => panic!("Unknown selector name: {}", selector_name),
    }
}

#[test]
#[cfg(feature = "cairo_native")]
fn test_builtin_counts_consistency() {
    let test_contract_casm =
        FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let chain_info = &ChainInfo::create_for_testing();
    let mut casm_state = test_state(chain_info, BALANCE, &[(test_contract_casm, 1)]);

    let entry_point_call_casm = CallEntryPoint {
        entry_point_selector: selector_from_name("test_builtin_counts_consistency"),
        calldata: calldata![],
        ..trivial_external_entry_point_new(test_contract_casm)
    };

    let casm_call_info = entry_point_call_casm.execute_directly(&mut casm_state).unwrap();
    assert!(!casm_call_info.execution.failed, "CASM execution failed, {:?}", casm_call_info);

    let expected_builtins = [
        BuiltinName::range_check,
        BuiltinName::pedersen,
        BuiltinName::poseidon,
        BuiltinName::keccak,
        BuiltinName::bitwise,
        BuiltinName::ec_op,
        BuiltinName::add_mod,
        BuiltinName::mul_mod,
        BuiltinName::range_check96,
    ];
    // Check that all builtins are covered by this test.
    for builtin in expected_builtins {
        assert!(
            casm_call_info.builtin_counters.get(&builtin).copied().unwrap_or(0) > 0,
            "Builtin {:?} was not called",
            builtin
        );
    }

    let test_contract_native =
        FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Native));
    let mut native_state = test_state(chain_info, BALANCE, &[(test_contract_native, 1)]);

    let entry_point_call_native = CallEntryPoint {
        entry_point_selector: selector_from_name("test_builtin_counts_consistency"),
        calldata: calldata![],
        ..trivial_external_entry_point_new(test_contract_native)
    };

    let native_call_info = entry_point_call_native.execute_directly(&mut native_state).unwrap();
    assert!(!native_call_info.execution.failed, "Native execution failed");

    // TODO(Einat): Uncomment this when native builtins are supported.
    let casm_builtins = &casm_call_info.builtin_counters;
    let native_builtins = &native_call_info.builtin_counters;
    assert_eq!(
        casm_builtins, native_builtins,
        "Builtin usage should be identical between CASM and Native"
    );
}
