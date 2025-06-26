use core::panic;
use std::sync::Arc;

use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use expect_test::expect;
use itertools::Itertools;
use pretty_assertions::assert_eq;
use rstest::rstest;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::execution_utils::format_panic_data;
use starknet_api::transaction::fields::Calldata;
use starknet_api::{calldata as calldata_macro, felt};
use test_case::test_case;

use crate::context::{BlockContext, ChainInfo};
use crate::execution::contract_class::TrackedResource;
use crate::execution::entry_point::CallEntryPoint;
use crate::retdata;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::syscall::build_recurse_calldata;
use crate::test_utils::{trivial_external_entry_point_new, CompilerBasedVersion, BALANCE};

#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native; "Native"))]
#[test_case(RunnableCairo1::Casm;"VM")]
fn test_call_contract_that_panics(runnable_version: RunnableCairo1) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let empty_contract = FeatureContract::Empty(CairoVersion::Cairo1(runnable_version));
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(test_contract, 1), (empty_contract, 0)]);

    let new_class_hash = empty_contract.get_class_hash();
    let to_panic = true.into();
    let outer_entry_point_selector = selector_from_name("test_call_contract_revert");
    let calldata = create_calldata(
        test_contract.get_instance_address(0),
        "test_revert_helper",
        &[new_class_hash.0, to_panic],
    );
    let entry_point_call = CallEntryPoint {
        entry_point_selector: outer_entry_point_selector,
        calldata,
        ..trivial_external_entry_point_new(test_contract)
    };

    let res = entry_point_call.execute_directly(&mut state).unwrap();
    assert!(!res.execution.failed);
    let [inner_call] = &res.inner_calls[..] else {
        panic!("Expected one inner call, got {:?}", res.inner_calls);
    };
    // The inner call should have failed.
    assert!(inner_call.execution.failed);
    assert_eq!(
        format_panic_data(&inner_call.execution.retdata.0),
        "0x746573745f7265766572745f68656c706572 ('test_revert_helper')"
    );
    assert!(inner_call.execution.events.is_empty());
    assert!(inner_call.execution.l2_to_l1_messages.is_empty());

    // Check that the tracked resource is SierraGas to make sure that Native is running.
    for call in res.iter() {
        assert_eq!(call.tracked_resource, TrackedResource::SierraGas);
        assert_eq!(call.execution.cairo_native, runnable_version.is_cairo_native());
    }
}

#[rstest]
#[cfg_attr(feature = "cairo_native", case::native(RunnableCairo1::Native))]
#[case::vm(RunnableCairo1::Casm)]
/// This test verifies the behavior of a contract call sequence with nested calls and state
/// assertions.
///
/// - Contract A calls Contract B and asserts that the state remains unchanged.
/// - Contract B calls Contract C and panics.
/// - Contract C modifies the state but does not panic.
///
/// The test ensures that:
/// 1. Contract A's state remains unaffected despite the modifications in Contract C.
/// 2. Contract B error as expected.
/// 3. Tracked resources are correctly identified as SierraGas in all calls.
fn test_call_contract_and_than_revert(#[case] runnable_version: RunnableCairo1) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let empty_contract = FeatureContract::Empty(CairoVersion::Cairo1(runnable_version));
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(test_contract, 1), (empty_contract, 0)]);

    // Arguments of Contact C.
    let new_class_hash = empty_contract.get_class_hash();
    let to_panic = false.into();

    // Calldata of contract B
    let middle_call_data = create_calldata(
        test_contract.get_instance_address(0),
        "test_revert_helper",
        &[new_class_hash.0, to_panic],
    );

    // Calldata of contract A
    let calldata = create_calldata(
        test_contract.get_instance_address(0),
        "middle_revert_contract",
        &middle_call_data.0,
    );

    // Create the entry point call to contract A.
    let outer_entry_point_selector = selector_from_name("test_call_contract_revert");
    let entry_point_call = CallEntryPoint {
        entry_point_selector: outer_entry_point_selector,
        calldata,
        ..trivial_external_entry_point_new(test_contract)
    };

    // Execute.
    let call_info_a = entry_point_call.execute_directly(&mut state).unwrap();

    // Contract A should not fail.
    assert!(!call_info_a.execution.failed);

    // Contract B should fail.
    let [inner_call_b] = &call_info_a.inner_calls[..] else {
        panic!("Expected one inner call, got {:?}", call_info_a.inner_calls);
    };
    assert!(inner_call_b.execution.failed);
    assert!(inner_call_b.execution.events.is_empty());
    assert!(inner_call_b.execution.l2_to_l1_messages.is_empty());
    assert_eq!(
        format_panic_data(&inner_call_b.execution.retdata.0),
        "0x657865637574655f616e645f726576657274 ('execute_and_revert')"
    );

    // Contract C should not fail.
    let [inner_inner_call_c] = &inner_call_b.inner_calls[..] else {
        panic!("Expected one inner call, got {:?}", inner_call_b.inner_calls);
    };
    assert!(!inner_inner_call_c.execution.failed);

    // Contract C events and messages should be reverted,
    // since his parent (contract B) panics.
    assert!(inner_inner_call_c.execution.events.is_empty());
    assert!(inner_inner_call_c.execution.l2_to_l1_messages.is_empty());

    // Check that the tracked resource is SierraGas to make sure that Native is running.
    for call in call_info_a.iter() {
        assert_eq!(call.tracked_resource, TrackedResource::SierraGas);
        assert_eq!(call.execution.cairo_native, runnable_version.is_cairo_native());
    }
}

#[rstest]
#[cfg_attr(feature = "cairo_native", case::native(RunnableCairo1::Native))]
#[case::vm(RunnableCairo1::Casm)]
/// This test verifies the behavior of a contract call with inner calls where both try to change
/// the storage, but one succeeds and the other fails (panics).
///
/// - Contract A call contact B.
/// - Contract B changes the storage value from 0 to 10.
/// - Contract A call contact C.
/// - Contract C changes the storage value from 10 to 17 and panics.
/// - Contract A checks that storage value == 10.
fn test_revert_with_inner_call_and_reverted_storage(#[case] runnable_version: RunnableCairo1) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let empty_contract = FeatureContract::Empty(CairoVersion::Cairo1(runnable_version));
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(test_contract, 1), (empty_contract, 0)]);

    // Calldata of contract A
    let calldata = Calldata(
        [test_contract.get_instance_address(0).into(), empty_contract.get_class_hash().0]
            .to_vec()
            .into(),
    );

    // Create the entry point call to contract A.
    let outer_entry_point_selector =
        selector_from_name("test_revert_with_inner_call_and_reverted_storage");
    let entry_point_call = CallEntryPoint {
        entry_point_selector: outer_entry_point_selector,
        calldata,
        ..trivial_external_entry_point_new(test_contract)
    };

    // Execute.
    let outer_call = entry_point_call.execute_directly(&mut state).unwrap();

    // The outer call (contract A) should not fail.
    assert!(!outer_call.execution.failed);

    let [inner_call_to_b, inner_call_to_c] = &outer_call.inner_calls[..] else {
        panic!("Expected two inner calls, got {:?}", outer_call.inner_calls);
    };

    // The first inner call (contract B) should not fail.
    assert!(inner_call_to_c.execution.failed);
    // The second inner call (contract C) should fail.
    assert!(!inner_call_to_b.execution.failed);

    // Check that the tracked resource is SierraGas to make sure that Native is running.
    for call in outer_call.iter() {
        assert_eq!(call.tracked_resource, TrackedResource::SierraGas);
        assert_eq!(call.execution.cairo_native, runnable_version.is_cairo_native());
    }
}

#[cfg_attr(
    feature = "cairo_native",
    test_case(
      FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Native)),
      FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Native));
      "Call Contract between two contracts using Native"
    )
)]
#[test_case(
    FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm)),
    FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
    "Call Contract between two contracts using VM"
)]
fn test_call_contract(outer_contract: FeatureContract, inner_contract: FeatureContract) {
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(outer_contract, 1), (inner_contract, 1)]);
    let value = felt!(48_u8);

    let outer_entry_point_selector = selector_from_name("test_call_contract");
    let calldata = create_calldata(
        inner_contract.get_instance_address(0),
        "test_storage_read_write",
        &[
            felt!(405_u16), // Calldata: storage address.
            value,          // Calldata: value.
        ],
    );
    let entry_point_call = CallEntryPoint {
        entry_point_selector: outer_entry_point_selector,
        calldata,
        ..trivial_external_entry_point_new(outer_contract)
    };

    let mut execution = entry_point_call.execute_directly(&mut state).unwrap().execution;
    assert_eq!(execution.cairo_native, outer_contract.cairo_version().is_cairo_native());
    execution.cairo_native = false; // For comparison.
    expect![[r#"
        CallExecution {
            retdata: Retdata(
                [
                    0x30,
                ],
            ),
            events: [],
            l2_to_l1_messages: [],
            cairo_native: false,
            failed: false,
            gas_consumed: 129870,
        }
    "#]]
    .assert_debug_eq(&execution);
    assert_eq!(execution.retdata, retdata![value]);
}
#[cfg_attr(
    feature = "cairo_native",
    test_case(
    RunnableCairo1::Native, true;
    "Call execute directly using native, `block_direct_execute_call` = true."
))]
#[cfg_attr(
    feature = "cairo_native",
    test_case(
    RunnableCairo1::Native, false;
    "Call execute directly using native, `block_direct_execute_call` = false."
))]
#[test_case(
    RunnableCairo1::Casm, true;
    "Call execute directly using VM, `block_direct_execute_call` = true."
)]
#[test_case(
    RunnableCairo1::Casm, false;
    "Call execute directly using VM, `block_direct_execute_call` = false."
)]
fn test_direct_execute_call(cairo1_type: RunnableCairo1, block_direct_execute_call: bool) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(cairo1_type));
    let contract_with_execute = FeatureContract::EmptyAccount(cairo1_type);
    let chain_info = &ChainInfo::create_for_testing();
    let mut state =
        test_state(chain_info, BALANCE, &[(test_contract, 1), (contract_with_execute, 1)]);

    let test_contract_address = *test_contract.get_instance_address(0).0.key();
    let contract_with_execute_address = *contract_with_execute.get_instance_address(0).0.key();
    let call_execute_directly_selector = selector_from_name("call_execute_directly");
    let return_result_selector = selector_from_name("return_result");

    let call_execute_directly = CallEntryPoint {
        entry_point_selector: call_execute_directly_selector,
        calldata: calldata_macro![
            // The Execute entrypoint of this contract will be called.
            contract_with_execute_address,
            // Outer calldata (passed to `execute` entrypoint)
            felt!(4_u8), // Outer calldata length.
            test_contract_address,
            return_result_selector.0,
            // Inner calldata (passed to function called by `execute` entrypoint)
            felt!(1_u8), // Inner calldata length.
            felt!(0_u8)  // Inner calldata value.
        ],
        ..trivial_external_entry_point_new(test_contract)
    };

    let mut block_context = BlockContext::create_for_testing();
    block_context.versioned_constants.block_direct_execute_call = block_direct_execute_call;
    let call_info = call_execute_directly
        .execute_directly_given_block_context(&mut state, block_context)
        .unwrap();

    if block_direct_execute_call {
        assert!(call_info.execution.failed, "Expected direct execute call to fail.");
        assert_eq!(
            format_panic_data(&call_info.execution.retdata.0),
            "0x496e76616c696420617267756d656e74 ('Invalid argument')",
        );
    } else {
        assert!(
            !call_info.execution.failed,
            "Expected direct execute call to succeed, because `block_direct_execute_call` is \
             false."
        );
    }
}

/// Cairo0 / Old Cairo1 / Cairo1 / Native calls to Cairo0 / Old Cairo1 / Cairo1 / Native.
#[cfg(feature = "cairo_native")]
#[rstest]
fn test_tracked_resources(
    #[values(
        CompilerBasedVersion::CairoVersion(CairoVersion::Cairo0),
        CompilerBasedVersion::OldCairo1,
        CompilerBasedVersion::CairoVersion(CairoVersion::Cairo1(RunnableCairo1::Casm)),
        CompilerBasedVersion::CairoVersion(CairoVersion::Cairo1(RunnableCairo1::Native))
    )]
    outer_version: CompilerBasedVersion,
    #[values(
        CompilerBasedVersion::CairoVersion(CairoVersion::Cairo0),
        CompilerBasedVersion::OldCairo1,
        CompilerBasedVersion::CairoVersion(CairoVersion::Cairo1(RunnableCairo1::Casm)),
        CompilerBasedVersion::CairoVersion(CairoVersion::Cairo1(RunnableCairo1::Native))
    )]
    inner_version: CompilerBasedVersion,
) {
    test_tracked_resources_fn(outer_version, inner_version);
}

/// Cairo0 / Old Cairo1 / Cairo1 calls to Cairo0 / Old Cairo1 / Cairo1.
#[cfg(not(feature = "cairo_native"))]
#[rstest]
fn test_tracked_resources(
    #[values(
        CompilerBasedVersion::CairoVersion(CairoVersion::Cairo0),
        CompilerBasedVersion::OldCairo1,
        CompilerBasedVersion::CairoVersion(CairoVersion::Cairo1(RunnableCairo1::Casm))
    )]
    outer_version: CompilerBasedVersion,
    #[values(
        CompilerBasedVersion::CairoVersion(CairoVersion::Cairo0),
        CompilerBasedVersion::OldCairo1,
        CompilerBasedVersion::CairoVersion(CairoVersion::Cairo1(RunnableCairo1::Casm))
    )]
    inner_version: CompilerBasedVersion,
) {
    test_tracked_resources_fn(outer_version, inner_version);
}

fn test_tracked_resources_fn(
    outer_version: CompilerBasedVersion,
    inner_version: CompilerBasedVersion,
) {
    let outer_contract = outer_version.get_test_contract();
    let inner_contract = inner_version.get_test_contract();
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(outer_contract, 1), (inner_contract, 1)]);

    let outer_entry_point_selector = selector_from_name("test_call_contract");
    let calldata = build_recurse_calldata(&[inner_version]);
    let entry_point_call = CallEntryPoint {
        entry_point_selector: outer_entry_point_selector,
        calldata,
        ..trivial_external_entry_point_new(outer_contract)
    };

    let execution = entry_point_call.execute_directly(&mut state).unwrap();
    let expected_outer_resource = outer_version.own_tracked_resource();
    assert_eq!(execution.tracked_resource, expected_outer_resource);

    // If the outer call uses CairoSteps, then use it for inner.
    // See execute_entry_point_call_wrapper in crates/blockifier/src/execution/execution_utils.rs
    let expected_inner_resource = if expected_outer_resource == inner_version.own_tracked_resource()
    {
        expected_outer_resource
    } else {
        TrackedResource::CairoSteps
    };

    assert_eq!(execution.inner_calls.first().unwrap().tracked_resource, expected_inner_resource);
}

#[test_case(CompilerBasedVersion::CairoVersion(CairoVersion::Cairo0), CompilerBasedVersion::CairoVersion(CairoVersion::Cairo1(RunnableCairo1::Casm)); "Cairo0_and_Cairo1")]
#[test_case(CompilerBasedVersion::OldCairo1, CompilerBasedVersion::CairoVersion(CairoVersion::Cairo1(RunnableCairo1::Casm)); "OldCairo1_and_Cairo1")]
#[cfg_attr(
  feature = "cairo_native",
  test_case(CompilerBasedVersion::CairoVersion(CairoVersion::Cairo0), CompilerBasedVersion::CairoVersion(CairoVersion::Cairo1(RunnableCairo1::Native)); "Cairo0_and_Native")
)]
#[cfg_attr(
  feature = "cairo_native",
  test_case(CompilerBasedVersion::OldCairo1, CompilerBasedVersion::CairoVersion(CairoVersion::Cairo1(RunnableCairo1::Native)); "OldCairo1_and_Native")
)]
fn test_tracked_resources_nested(
    cairo_steps_contract_version: CompilerBasedVersion,
    sierra_gas_contract_version: CompilerBasedVersion,
) {
    let cairo_steps_contract = cairo_steps_contract_version.get_test_contract();
    let sierra_gas_contract = sierra_gas_contract_version.get_test_contract();
    let chain_info = &ChainInfo::create_for_testing();
    let mut state =
        test_state(chain_info, BALANCE, &[(sierra_gas_contract, 1), (cairo_steps_contract, 1)]);

    let first_calldata =
        build_recurse_calldata(&[cairo_steps_contract_version, sierra_gas_contract_version]);

    let second_calldata = build_recurse_calldata(&[sierra_gas_contract_version]);

    let concatenated_calldata_felts = [first_calldata.0, second_calldata.0]
        .into_iter()
        .map(|calldata_felts| calldata_felts.iter().copied().collect_vec())
        .concat();
    let concatenated_calldata = Calldata(Arc::new(concatenated_calldata_felts));
    let call_contract_selector = selector_from_name("test_call_two_contracts");
    let entry_point_call = CallEntryPoint {
        entry_point_selector: call_contract_selector,
        calldata: concatenated_calldata,
        ..trivial_external_entry_point_new(sierra_gas_contract)
    };
    let main_call_info = entry_point_call.execute_directly(&mut state).unwrap();

    assert_eq!(main_call_info.tracked_resource, TrackedResource::SierraGas);
    assert_ne!(main_call_info.execution.gas_consumed, 0);

    let first_inner_call = main_call_info.inner_calls.first().unwrap();
    assert_eq!(first_inner_call.tracked_resource, TrackedResource::CairoSteps);
    assert_eq!(first_inner_call.execution.gas_consumed, 0);
    assert_eq!(first_inner_call.execution.cairo_native, false);
    let inner_inner_call = first_inner_call.inner_calls.first().unwrap();
    assert_eq!(inner_inner_call.tracked_resource, TrackedResource::CairoSteps);
    assert_eq!(inner_inner_call.execution.gas_consumed, 0);
    assert_eq!(inner_inner_call.execution.cairo_native, false);

    let second_inner_call = main_call_info.inner_calls.get(1).unwrap();
    assert_eq!(second_inner_call.tracked_resource, TrackedResource::SierraGas);
    assert_ne!(second_inner_call.execution.gas_consumed, 0);
    assert_eq!(
        second_inner_call.execution.cairo_native,
        sierra_gas_contract_version.is_cairo_native()
    );
}

#[rstest]
#[case(RunnableCairo1::Casm)]
#[cfg_attr(feature = "cairo_native", case(RunnableCairo1::Native))]
fn test_empty_function_flow(#[case] runnable: RunnableCairo1) {
    let outer_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable));
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(outer_contract, 1)]);
    let test_contract_address = outer_contract.get_instance_address(0);

    let calldata = create_calldata(
        test_contract_address,
        "empty_function",
        &[], // Calldata.
    );
    let outer_entry_point_selector = selector_from_name("test_call_contract");
    let entry_point_call = CallEntryPoint {
        entry_point_selector: outer_entry_point_selector,
        calldata,
        ..trivial_external_entry_point_new(outer_contract)
    };

    let call_info = entry_point_call.execute_directly(&mut state).unwrap();

    // Contract should not fail.
    assert!(!call_info.execution.failed);
}
