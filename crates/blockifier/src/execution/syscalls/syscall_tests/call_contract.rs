use core::panic;
use std::sync::Arc;

use itertools::Itertools;
use pretty_assertions::assert_eq;
use rstest::rstest;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::execution_utils::format_panic_data;
use starknet_api::felt;
use starknet_api::transaction::fields::Calldata;
use test_case::test_case;

use super::constants::REQUIRED_GAS_CALL_CONTRACT_TEST;
use crate::context::ChainInfo;
use crate::execution::call_info::CallExecution;
use crate::execution::contract_class::TrackedResource;
use crate::execution::entry_point::CallEntryPoint;
use crate::retdata;
use crate::test_utils::contracts::FeatureContract;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::syscall::build_recurse_calldata;
use crate::test_utils::{
    create_calldata,
    trivial_external_entry_point_new,
    CairoVersion,
    CompilerBasedVersion,
    BALANCE,
};

// TODO: Add test for native once reverts are supported.
#[test]
fn test_call_contract_that_panics() {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1);
    let empty_contract = FeatureContract::Empty(CairoVersion::Cairo1);
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(test_contract, 1), (empty_contract, 0)]);

    let new_class_hash = empty_contract.get_class_hash();
    let outer_entry_point_selector = selector_from_name("test_call_contract_revert");
    let calldata = create_calldata(
        FeatureContract::TestContract(CairoVersion::Cairo1).get_instance_address(0),
        "test_revert_helper",
        &[new_class_hash.0],
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
}

#[cfg_attr(
    feature = "cairo_native",
    test_case(
      FeatureContract::TestContract(CairoVersion::Native),
      FeatureContract::TestContract(CairoVersion::Native),
      190370;
      "Call Contract between two contracts using Native"
    )
)]
#[test_case(
    FeatureContract::TestContract(CairoVersion::Cairo1),
    FeatureContract::TestContract(CairoVersion::Cairo1),
    REQUIRED_GAS_CALL_CONTRACT_TEST;
    "Call Contract between two contracts using VM"
)]
fn test_call_contract(
    outer_contract: FeatureContract,
    inner_contract: FeatureContract,
    expected_gas: u64,
) {
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(outer_contract, 1), (inner_contract, 1)]);

    let outer_entry_point_selector = selector_from_name("test_call_contract");
    let calldata = create_calldata(
        inner_contract.get_instance_address(0),
        "test_storage_read_write",
        &[
            felt!(405_u16), // Calldata: storage address.
            felt!(48_u8),   // Calldata: value.
        ],
    );
    let entry_point_call = CallEntryPoint {
        entry_point_selector: outer_entry_point_selector,
        calldata,
        ..trivial_external_entry_point_new(outer_contract)
    };

    assert_eq!(
        entry_point_call.execute_directly(&mut state).unwrap().execution,
        CallExecution {
            retdata: retdata![felt!(48_u8)],
            gas_consumed: expected_gas,
            ..CallExecution::default()
        }
    );
}

/// Cairo0 / Old Cairo1 / Cairo1 / Native calls to Cairo0 / Old Cairo1 / Cairo1 / Native.
#[cfg(feature = "cairo_native")]
#[rstest]
fn test_tracked_resources(
    #[values(
        CompilerBasedVersion::CairoVersion(CairoVersion::Cairo0),
        CompilerBasedVersion::OldCairo1,
        CompilerBasedVersion::CairoVersion(CairoVersion::Cairo1),
        CompilerBasedVersion::CairoVersion(CairoVersion::Native)
    )]
    outer_version: CompilerBasedVersion,
    #[values(
        CompilerBasedVersion::CairoVersion(CairoVersion::Cairo0),
        CompilerBasedVersion::OldCairo1,
        CompilerBasedVersion::CairoVersion(CairoVersion::Cairo1),
        CompilerBasedVersion::CairoVersion(CairoVersion::Native)
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
        CompilerBasedVersion::CairoVersion(CairoVersion::Cairo1)
    )]
    outer_version: CompilerBasedVersion,
    #[values(
        CompilerBasedVersion::CairoVersion(CairoVersion::Cairo0),
        CompilerBasedVersion::OldCairo1,
        CompilerBasedVersion::CairoVersion(CairoVersion::Cairo1)
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

#[test_case(CompilerBasedVersion::CairoVersion(CairoVersion::Cairo0), CompilerBasedVersion::CairoVersion(CairoVersion::Cairo1); "Cairo0_and_Cairo1")]
#[test_case(CompilerBasedVersion::OldCairo1, CompilerBasedVersion::CairoVersion(CairoVersion::Cairo1); "OldCairo1_and_Cairo1")]
#[cfg_attr(
  feature = "cairo_native",
  test_case(CompilerBasedVersion::CairoVersion(CairoVersion::Cairo0), CompilerBasedVersion::CairoVersion(CairoVersion::Native); "Cairo0_and_Native")
)]
#[cfg_attr(
  feature = "cairo_native",
  test_case(CompilerBasedVersion::OldCairo1, CompilerBasedVersion::CairoVersion(CairoVersion::Native); "OldCairo1_and_Native")
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

    let concated_calldata_felts = [first_calldata.0, second_calldata.0]
        .into_iter()
        .map(|calldata_felts| calldata_felts.iter().copied().collect_vec())
        .concat();
    let concated_calldata = Calldata(Arc::new(concated_calldata_felts));
    let call_contract_selector = selector_from_name("test_call_two_contracts");
    let entry_point_call = CallEntryPoint {
        entry_point_selector: call_contract_selector,
        calldata: concated_calldata,
        ..trivial_external_entry_point_new(sierra_gas_contract)
    };
    let execution = entry_point_call.execute_directly(&mut state).unwrap();

    assert_eq!(execution.tracked_resource, TrackedResource::SierraGas);
    let first_call_info = execution.inner_calls.first().unwrap();
    assert_eq!(first_call_info.tracked_resource, TrackedResource::CairoSteps);
    assert_eq!(
        first_call_info.inner_calls.first().unwrap().tracked_resource,
        TrackedResource::CairoSteps
    );

    let second_inner_call_info = execution.inner_calls.get(1).unwrap();
    assert_eq!(second_inner_call_info.tracked_resource, TrackedResource::SierraGas);
}
