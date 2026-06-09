use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use expect_test::expect;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::felt;
use starknet_api::transaction::fields::Calldata;
use starknet_types_core::felt::Felt;
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

/// Runs `entry_point` (with `calldata`) and asserts the secp256r1 zero-x point was rejected with a
/// hard error (it must not be a catchable revert, otherwise the contract could still use the
/// point).
fn assert_secp256r1_zero_x_point_rejected(
    runnable_version: RunnableCairo1,
    entry_point: &str,
    calldata: Vec<Felt>,
) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(test_contract, 1)]);

    let entry_point_call = CallEntryPoint {
        calldata: Calldata(calldata.into()),
        entry_point_selector: selector_from_name(entry_point),
        ..trivial_external_entry_point_new(test_contract)
    };
    let error = entry_point_call.execute_directly(&mut state).unwrap_err();
    assert!(
        error.to_string().contains("secp256r1 points with x-coordinate 0 are not allowed"),
        "Unexpected error: {error}"
    );
}

// secp256r1 has a valid affine point with x == 0 and y != 0, which the OS handles inconsistently
// (it treats x == 0 as the point at infinity). The following tests verify that this point is
// rejected with a hard error regardless of which secp256r1 syscall produces it, since they all
// funnel through `allocate_point`. u256 arguments are passed as (low, high) 128-bit limbs.

/// The valid secp256r1 point with x == 0; y is a square root of the curve's `b`.
const ZERO_X_POINT_Y_LOW: &str = "0x541c2af31dae871728bf856a174f93f4";
const ZERO_X_POINT_Y_HIGH: &str = "0x66485c780e2f83d72433bd5d84a06bb6";

/// `secp256r1_new` with the zero-x point.
#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native; "Native"))]
#[test_case(RunnableCairo1::Casm; "VM")]
fn test_secp256r1_new_zero_x_point_rejected(runnable_version: RunnableCairo1) {
    // `test_getter_secp256r1` calls `secp256r1_new_syscall(x, y)`.
    let calldata =
        vec![felt!("0x0"), felt!("0x0"), felt!(ZERO_X_POINT_Y_LOW), felt!(ZERO_X_POINT_Y_HIGH)];
    assert_secp256r1_zero_x_point_rejected(runnable_version, "test_getter_secp256r1", calldata);
}

/// `secp256r1_get_point_from_x` with x == 0.
#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native; "Native"))]
#[test_case(RunnableCairo1::Casm; "VM")]
fn test_secp256r1_get_point_from_x_zero_x_point_rejected(runnable_version: RunnableCairo1) {
    // `test_new_point_secp256r1(x)` calls `secp256r1_get_point_from_x_syscall(x, ..)`; with x == 0
    // it derives the zero-x point.
    let calldata = vec![felt!("0x0"), felt!("0x0")];
    assert_secp256r1_zero_x_point_rejected(runnable_version, "test_new_point_secp256r1", calldata);
}

/// `secp256r1_add` producing the zero-x point (p0 = zero_x_point - generator, so p0 + generator
/// lands on the zero-x point).
#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native; "Native"))]
#[test_case(RunnableCairo1::Casm; "VM")]
fn test_secp256r1_add_zero_x_point_rejected(runnable_version: RunnableCairo1) {
    // `test_add_secp256r1(x, y)` computes `secp256r1_new(x, y) + generator`.
    let calldata = vec![
        felt!("0x6211691a4a6c250a993ae2a93a66db1f"),
        felt!("0xfd92d626016b41a6cad9cd497a821cc0"),
        felt!("0x3baa0c70538a1a79a3669e9c26191d08"),
        felt!("0x4dc1e96ddb4fb7f4cd7f08b7f2085bcb"),
    ];
    assert_secp256r1_zero_x_point_rejected(runnable_version, "test_add_secp256r1", calldata);
}

/// `secp256r1_mul` producing the zero-x point (p = zero_x_point / 2, so 2 * p lands on it).
#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native; "Native"))]
#[test_case(RunnableCairo1::Casm; "VM")]
fn test_secp256r1_mul_zero_x_point_rejected(runnable_version: RunnableCairo1) {
    // `test_mul_point_secp256r1(x, y, scalar)` computes `scalar * secp256r1_new(x, y)`.
    let calldata = vec![
        felt!("0x278e28febff3b05632eeff09011c5579"),
        felt!("0x81bfb55b010b1bdf08b8d9d8590087aa"),
        felt!("0x50799b354b0fb1e77eb75eba8bff3d58"),
        felt!("0x8cd2f199d9815d7585073034eb76c93d"),
        felt!("0x2"),
        felt!("0x0"),
    ];
    assert_secp256r1_zero_x_point_rejected(runnable_version, "test_mul_point_secp256r1", calldata);
}
