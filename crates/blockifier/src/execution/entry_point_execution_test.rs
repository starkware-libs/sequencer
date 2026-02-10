use std::sync::Arc;

use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use rstest::rstest;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::execution_resources::GasAmount;
use starknet_api::transaction::fields::Calldata;

use crate::context::ChainInfo;
use crate::execution::call_info::{CallInfo, ExtendedExecutionResources};
use crate::execution::contract_class::TrackedResource;
use crate::execution::entry_point::CallEntryPoint;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::syscall::build_recurse_calldata;
use crate::test_utils::{trivial_external_entry_point_new, CompilerBasedVersion, BALANCE};

/// Asserts that the charged resources of a call is consistent with the inner calls in its subtree.
fn assert_charged_resource_as_expected_rec(call_info: &CallInfo) {
    let inner_calls = &call_info.inner_calls;
    let mut children_vm_resources = ExtendedExecutionResources::default();
    let mut children_gas = GasAmount(0);
    for child_call_info in inner_calls.iter() {
        let gas_consumed = GasAmount(child_call_info.execution.gas_consumed);
        let vm_resources = &child_call_info.resources;
        children_vm_resources += vm_resources;
        children_gas += gas_consumed;
    }

    let gas_consumed = GasAmount(call_info.execution.gas_consumed);
    let vm_resources = &call_info.resources;

    match call_info.tracked_resource {
        TrackedResource::SierraGas => {
            assert_eq!(vm_resources, &children_vm_resources);
            assert!(gas_consumed > children_gas)
        }
        TrackedResource::CairoSteps => {
            assert_eq!(gas_consumed, children_gas);
            assert!(vm_resources.vm_resources.n_steps > children_vm_resources.vm_resources.n_steps)
        }
    }

    for child_call_info in inner_calls.iter() {
        assert_charged_resource_as_expected_rec(child_call_info);
    }
}

#[rstest]
fn test_charged_resources_computation(
    #[values(
        CompilerBasedVersion::CairoVersion(CairoVersion::Cairo0),
        CompilerBasedVersion::OldCairo1
    )]
    third_contract_version: CompilerBasedVersion,
    #[values(
        CompilerBasedVersion::CairoVersion(CairoVersion::Cairo0),
        CompilerBasedVersion::OldCairo1
    )]
    fourth_contract_version: CompilerBasedVersion,
    #[values(
        CompilerBasedVersion::CairoVersion(CairoVersion::Cairo0),
        CompilerBasedVersion::OldCairo1
    )]
    second_branch_contract_version: CompilerBasedVersion,
) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let chain_info = &ChainInfo::create_for_testing();
    let contracts = CompilerBasedVersion::iter().map(|version| version.get_test_contract());
    let mut state = test_state(
        chain_info,
        BALANCE,
        &contracts.map(|contract| (contract, 1)).collect::<Vec<_>>(),
    );
    let call_versions = [
        CompilerBasedVersion::CairoVersion(CairoVersion::Cairo1(RunnableCairo1::Casm)),
        CompilerBasedVersion::CairoVersion(CairoVersion::Cairo1(RunnableCairo1::Casm)),
        third_contract_version,
        fourth_contract_version,
    ];

    let first_calldata = build_recurse_calldata(&call_versions);
    let second_calldata = build_recurse_calldata(&[second_branch_contract_version]);
    let outer_calldata = Calldata(Arc::new(
        (*first_calldata.0).iter().copied().chain((*second_calldata.0).iter().copied()).collect(),
    ));
    let call_contract_selector = selector_from_name("test_call_two_contracts");
    let entry_point_call = CallEntryPoint {
        entry_point_selector: call_contract_selector,
        calldata: outer_calldata,
        ..trivial_external_entry_point_new(test_contract)
    };
    let call_info = entry_point_call.execute_directly(&mut state).unwrap();

    assert_charged_resource_as_expected_rec(&call_info);
}
