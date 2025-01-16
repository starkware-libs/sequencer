use std::sync::Arc;

use rstest::rstest;
use rstest_reuse::apply;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::transaction::fields::Calldata;

use crate::context::{BlockContext, ChainInfo};
use crate::execution::entry_point::CallEntryPoint;
use crate::test_utils::contracts::FeatureContract;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::test_templates::runnable_version;
use crate::test_utils::{trivial_external_entry_point_new, CairoVersion, RunnableCairo1, BALANCE};

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

    let calldata = Calldata(vec![].into());
    let entry_point_call = CallEntryPoint {
        entry_point_selector: selector_from_name(selector_name),
        calldata,
        ..trivial_external_entry_point_new(test_contract)
    };

    let mut block_context = BlockContext::create_for_account_testing();
    change_builtins_gas_cost(&mut block_context, selector_name);

    let call_info_while_tracking_vm_resources =
        entry_point_call.execute_directly_with_block_context(&mut state, block_context).unwrap();

    assert!(call_info_while_tracking_vm_resources.execution.gas_consumed >= 1000000);
}

fn change_builtins_gas_cost(block_context: &mut BlockContext, selector_name: &str) {
    let os_constants = Arc::make_mut(&mut block_context.versioned_constants.os_constants);
    match selector_name {
        "test_pedersen" => {
            os_constants.gas_costs.builtins.pedersen = 10000000;
        }
        "test_bitwise" => {
            os_constants.gas_costs.builtins.bitwise = 10000000;
        }
        "test_ecop" => {
            os_constants.gas_costs.builtins.ecop = 10000000;
        }
        "test_poseidon" => {
            os_constants.gas_costs.builtins.poseidon = 10000000;
        }
        "test_circuit" => {
            os_constants.gas_costs.builtins.add_mod = 10000000;
            os_constants.gas_costs.builtins.mul_mod = 10000000;
        }
        _ => panic!("Unknown selector name: {}", selector_name),
    }
}
