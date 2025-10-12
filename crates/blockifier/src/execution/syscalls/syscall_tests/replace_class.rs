use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use expect_test::expect;
use pretty_assertions::assert_eq;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::{calldata, felt};
use test_case::test_case;

use crate::context::ChainInfo;
use crate::execution::entry_point::CallEntryPoint;
use crate::state::state_api::StateReader;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::{trivial_external_entry_point_new, BALANCE};

#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native; "Native"))]
#[test_case(RunnableCairo1::Casm; "VM")]
fn undeclared_class_hash(runnable_version: RunnableCairo1) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let mut state = test_state(&ChainInfo::create_for_testing(), BALANCE, &[(test_contract, 1)]);

    let entry_point_call = CallEntryPoint {
        calldata: calldata![felt!(1234_u16)],
        entry_point_selector: selector_from_name("test_replace_class"),
        ..trivial_external_entry_point_new(test_contract)
    };
    let error = entry_point_call.execute_directly(&mut state).unwrap_err();

    assert!(error.to_string().contains("is not declared"));
}

#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native; "Native"))]
#[test_case(RunnableCairo1::Casm; "VM")]
fn cairo0_class_hash(runnable_version: RunnableCairo1) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let empty_contract_cairo0 = FeatureContract::Empty(CairoVersion::Cairo0);
    let mut state = test_state(
        &ChainInfo::create_for_testing(),
        BALANCE,
        &[(test_contract, 1), (empty_contract_cairo0, 0)],
    );

    // Replace with Cairo 0 class hash.
    let v0_class_hash = empty_contract_cairo0.get_class_hash();

    let entry_point_call = CallEntryPoint {
        calldata: calldata![v0_class_hash.0],
        entry_point_selector: selector_from_name("test_replace_class"),
        ..trivial_external_entry_point_new(test_contract)
    };
    let error = entry_point_call.execute_directly(&mut state).unwrap_err();

    assert!(error.to_string().contains("Cannot replace V1 class hash with V0 class hash"));
}

#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native; "Native"))]
#[test_case(RunnableCairo1::Casm; "VM")]
fn positive_flow(runnable_version: RunnableCairo1) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let empty_contract = FeatureContract::Empty(CairoVersion::Cairo1(runnable_version));
    let empty_contract_cairo0 = FeatureContract::Empty(CairoVersion::Cairo0);
    let mut state = test_state(
        &ChainInfo::create_for_testing(),
        BALANCE,
        &[(test_contract, 1), (empty_contract, 0), (empty_contract_cairo0, 0)],
    );
    let contract_address = test_contract.get_instance_address(0);

    let old_class_hash = test_contract.get_class_hash();
    let new_class_hash = empty_contract.get_class_hash();
    assert_eq!(state.get_class_hash_at(contract_address).unwrap(), old_class_hash);
    let entry_point_call = CallEntryPoint {
        calldata: calldata![new_class_hash.0],
        entry_point_selector: selector_from_name("test_replace_class"),
        ..trivial_external_entry_point_new(test_contract)
    };
    let mut execution = entry_point_call.execute_directly(&mut state).unwrap().execution;
    assert_eq!(execution.cairo_native, runnable_version.is_cairo_native());
    execution.cairo_native = false; // For comparison purposes.
    expect![[r#"
        CallExecution {
            retdata: Retdata(
                [],
            ),
            events: [],
            l2_to_l1_messages: [],
            cairo_native: false,
            failed: false,
            gas_consumed: 14920,
        }
    "#]]
    .assert_debug_eq(&execution);
    assert_eq!(state.get_class_hash_at(contract_address).unwrap(), new_class_hash);
}
