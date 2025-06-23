use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use expect_test::expect;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::transaction::fields::Calldata;
use test_case::test_case;

use crate::context::ChainInfo;
use crate::execution::entry_point::CallEntryPoint;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::{trivial_external_entry_point_new, BALANCE};

#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native; "Native"))]
#[test_case(RunnableCairo1::Casm; "VM")]
fn test_secp256k1(runnable_version: RunnableCairo1) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(test_contract, 1)]);

    let calldata = Calldata(vec![].into());
    let entry_point_call = CallEntryPoint {
        entry_point_selector: selector_from_name("test_secp256k1"),
        calldata,
        ..trivial_external_entry_point_new(test_contract)
    };

<<<<<<< HEAD
    let expectation = expect![[r#"
        CallExecution {
            retdata: Retdata(
                [],
            ),
            events: [],
            l2_to_l1_messages: [],
            failed: false,
            gas_consumed: 16997129,
        }
    "#]];
    expectation.assert_debug_eq(&entry_point_call.execute_directly(&mut state).unwrap().execution);
}

#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native; "Native"))]
#[test_case(RunnableCairo1::Casm; "VM")]
fn test_secp256k1_point_from_x(runnable_version: RunnableCairo1) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(test_contract, 1)]);

    let calldata = Calldata(vec![].into());
    let entry_point_call = CallEntryPoint {
        entry_point_selector: selector_from_name("test_secp256k1_point_from_x"),
        calldata,
        ..trivial_external_entry_point_new(test_contract)
    };

    expect![[r#"
        CallExecution {
            retdata: Retdata(
                [],
            ),
            events: [],
            l2_to_l1_messages: [],
            failed: false,
            gas_consumed: 184690,
        }
    "#]]
    .assert_debug_eq(&entry_point_call.execute_directly(&mut state).unwrap().execution);
||||||| 787b8bea3
    pretty_assertions::assert_eq!(
        entry_point_call.execute_directly(&mut state).unwrap().execution,
        CallExecution { gas_consumed: 17011779, ..Default::default() }
    );
=======
    pretty_assertions::assert_eq!(
        entry_point_call.execute_directly(&mut state).unwrap().execution,
        CallExecution {
            gas_consumed: 17011779,
            cairo_native: runnable_version.is_cairo_native(),
            ..Default::default()
        }
    );
>>>>>>> origin/main-v0.13.6
}

#[cfg_attr(feature = "cairo_native",test_case(RunnableCairo1::Native; "Native"))]
#[test_case(RunnableCairo1::Casm; "VM")]
fn test_secp256r1(runnable_version: RunnableCairo1) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(test_contract, 1)]);

    let calldata = Calldata(vec![].into());
    let entry_point_call = CallEntryPoint {
        entry_point_selector: selector_from_name("test_secp256r1"),
        calldata,
        ..trivial_external_entry_point_new(test_contract)
    };

<<<<<<< HEAD
    expect![[r#"
        CallExecution {
            retdata: Retdata(
                [],
            ),
            events: [],
            l2_to_l1_messages: [],
            failed: false,
            gas_consumed: 27569930,
        }
    "#]]
    .assert_debug_eq(&entry_point_call.execute_directly(&mut state).unwrap().execution);
||||||| 787b8bea3
    pretty_assertions::assert_eq!(
        entry_point_call.execute_directly(&mut state).unwrap().execution,
        CallExecution { gas_consumed: 27571210, ..Default::default() }
    );
=======
    pretty_assertions::assert_eq!(
        entry_point_call.execute_directly(&mut state).unwrap().execution,
        CallExecution {
            gas_consumed: 27571210,
            cairo_native: runnable_version.is_cairo_native(),
            ..Default::default()
        }
    );
>>>>>>> origin/main-v0.13.6
}
