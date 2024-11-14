use std::sync::Arc;

use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use rstest::rstest;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::execution_resources::GasAmount;
use starknet_api::transaction::fields::Calldata;

use crate::context::ChainInfo;
use crate::execution::call_info::{CallExecution, CallInfo, ChargedResources};
use crate::execution::contract_class::TrackedResource;
use crate::execution::entry_point::CallEntryPoint;
use crate::execution::entry_point_execution::gas_consumed_without_inner_calls;
use crate::test_utils::contracts::FeatureContract;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::syscall::build_recurse_calldata;
use crate::test_utils::{
    trivial_external_entry_point_new,
    CairoVersion,
    CompilerBasedVersion,
    BALANCE,
};

#[test]
/// Verifies that every call from the inner most to the outer has the expected gas_for_fee for the
/// following topology (marked as TrackedResource(gas_consumed)):
//       Gas(8) -> Gas(3) -> VM(2) -> VM(1)
//            \ -> VM(4)
// Expected values are 1 -> 1 -> 0 -> 0.
//                      \-> 0.
fn test_gas_for_fee() {
    // First branch - 3 nested calls.
    let mut inner_calls = vec![];
    for (tracked_resource, gas_consumed, expected_gas_for_fee) in [
        (TrackedResource::CairoSteps, 1, 0),
        (TrackedResource::CairoSteps, 2, 0),
        (TrackedResource::SierraGas, 3, 1),
    ] {
        assert_eq!(
            gas_consumed_without_inner_calls(&tracked_resource, gas_consumed, &inner_calls).0,
            expected_gas_for_fee
        );
        inner_calls = vec![CallInfo {
            execution: CallExecution { gas_consumed, ..Default::default() },
            tracked_resource,
            inner_calls,
            charged_resources: ChargedResources {
                gas_for_fee: GasAmount(expected_gas_for_fee),
                ..Default::default()
            },
            ..Default::default()
        }];
    }

    // Second branch - 1 call.
    let (tracked_resource, gas_consumed, expected_gas_for_fee) =
        (TrackedResource::CairoSteps, 4, 0);
    assert_eq!(
        gas_consumed_without_inner_calls(&tracked_resource, gas_consumed, &[]).0,
        expected_gas_for_fee
    );

    inner_calls.push(CallInfo {
        execution: CallExecution { gas_consumed, ..Default::default() },
        tracked_resource,
        charged_resources: ChargedResources {
            gas_for_fee: GasAmount(expected_gas_for_fee),
            ..Default::default()
        },
        ..Default::default()
    });

    // Outer call.
    assert_eq!(gas_consumed_without_inner_calls(&TrackedResource::SierraGas, 8, &inner_calls).0, 1);
}

/// Asserts that the charged resources of a call is consistent with the inner calls in its subtree.
fn assert_charged_resource_as_expected_rec(call_info: &CallInfo) {
    let inner_calls = &call_info.inner_calls;
    let mut children_vm_resources = ExecutionResources::default();
    let mut children_gas = GasAmount(0);
    for child_call_info in inner_calls.iter() {
        let ChargedResources { gas_for_fee, vm_resources } = &child_call_info.charged_resources;
        children_vm_resources += vm_resources;
        children_gas += *gas_for_fee;
    }

    let ChargedResources { gas_for_fee, vm_resources } = &call_info.charged_resources;

    match call_info.tracked_resource {
        TrackedResource::SierraGas => {
            assert_eq!(vm_resources, &children_vm_resources);
            assert!(gas_for_fee > &children_gas)
        }
        TrackedResource::CairoSteps => {
            assert_eq!(gas_for_fee, &children_gas);
            assert!(vm_resources.n_steps > children_vm_resources.n_steps)
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
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1);
    let chain_info = &ChainInfo::create_for_testing();
    let contracts = CompilerBasedVersion::iter().map(|version| version.get_test_contract());
    let mut state = test_state(
        chain_info,
        BALANCE,
        &contracts.map(|contract| (contract, 1)).collect::<Vec<_>>(),
    );
    let call_versions = [
        CompilerBasedVersion::CairoVersion(CairoVersion::Cairo1),
        CompilerBasedVersion::CairoVersion(CairoVersion::Cairo1),
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
