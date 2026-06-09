use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use expect_test::expect;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::felt;
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

    let mut execution = entry_point_call.execute_directly(&mut state).unwrap().execution;
    assert_eq!(execution.cairo_native, runnable_version.is_cairo_native());
    execution.cairo_native = false;

    let expectation = expect![[r#"
        CallExecution {
            retdata: Retdata(
                [],
            ),
            events: [],
            l2_to_l1_messages: [],
            cairo_native: false,
            failed: false,
            gas_consumed: 17008649,
        }
    "#]];
    expectation.assert_debug_eq(&execution);
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

    let mut execution = entry_point_call.execute_directly(&mut state).unwrap().execution;
    assert_eq!(execution.cairo_native, runnable_version.is_cairo_native());
    execution.cairo_native = false;

    expect![[r#"
        CallExecution {
            retdata: Retdata(
                [],
            ),
            events: [],
            l2_to_l1_messages: [],
            cairo_native: false,
            failed: false,
            gas_consumed: 183960,
        }
    "#]]
    .assert_debug_eq(&execution);
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
    let mut execution = entry_point_call.execute_directly(&mut state).unwrap().execution;
    assert_eq!(execution.cairo_native, runnable_version.is_cairo_native());
    execution.cairo_native = false;

    expect![[r#"
        CallExecution {
            retdata: Retdata(
                [],
            ),
            events: [],
            l2_to_l1_messages: [],
            cairo_native: false,
            failed: false,
            gas_consumed: 27578890,
        }
    "#]]
    .assert_debug_eq(&execution);
}

/// secp256r1 has a valid affine point with x == 0 and y != 0, which the OS handles inconsistently
/// (it treats x == 0 as the point at infinity). Verify that creating this point is rejected with a
/// hard error (it must not be a catchable revert, otherwise the contract could still use the
/// point).
#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native; "Native"))]
#[test_case(RunnableCairo1::Casm; "VM")]
fn test_secp256r1_zero_x_point_rejected(runnable_version: RunnableCairo1) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(test_contract, 1)]);

    // The valid secp256r1 point with x == 0: x == 0 and y is a square root of the curve's `b`.
    // u256 arguments are passed as (low, high) 128-bit limbs.
    let x_low = felt!("0x0");
    let x_high = felt!("0x0");
    let y_low = felt!("0x541c2af31dae871728bf856a174f93f4");
    let y_high = felt!("0x66485c780e2f83d72433bd5d84a06bb6");

    // `test_getter_secp256r1` calls `secp256r1_new_syscall(x, y)` with these coordinates.
    let entry_point_call = CallEntryPoint {
        calldata: Calldata(vec![x_low, x_high, y_low, y_high].into()),
        entry_point_selector: selector_from_name("test_getter_secp256r1"),
        ..trivial_external_entry_point_new(test_contract)
    };
    let error = entry_point_call.execute_directly(&mut state).unwrap_err();
    assert!(
        error.to_string().contains("secp256r1 points with x-coordinate 0 are not allowed"),
        "Unexpected error: {error}"
    );
}
