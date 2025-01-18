use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::{calldata, felt};
use test_case::test_case;

use crate::abi::constants::MAX_POSSIBLE_SIERRA_GAS;
#[cfg(feature = "cairo_native")]
use crate::context::ChainInfo;
use crate::execution::call_info::CallExecution;
use crate::execution::entry_point::CallEntryPoint;
use crate::execution::syscalls::syscall_tests::constants;
use crate::execution::syscalls::syscall_tests::get_block_hash::initialize_state;
use crate::execution::syscalls::SyscallSelector;
use crate::retdata;
use crate::test_utils::contracts::FeatureContract;
#[cfg(feature = "cairo_native")]
use crate::test_utils::initial_test_state::test_state;
#[cfg(feature = "cairo_native")]
use crate::test_utils::BALANCE;
use crate::test_utils::{trivial_external_entry_point_new, CairoVersion, RunnableCairo1};
use crate::versioned_constants::VersionedConstants;

#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native; "Native"))]
#[test_case(RunnableCairo1::Casm; "VM")]
fn test_out_of_gas(runnable_version: RunnableCairo1) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let (mut state, block_number, _block_hash) = initialize_state(test_contract);

    let calldata = calldata![block_number];
    let entry_point_call = CallEntryPoint {
        entry_point_selector: selector_from_name("test_get_block_hash"),
        calldata,
        initial_gas: constants::REQUIRED_GAS_GET_BLOCK_HASH_TEST - 1,
        ..trivial_external_entry_point_new(test_contract)
    };

    let gas_costs = &VersionedConstants::create_for_testing().os_constants.gas_costs;
    let get_block_hash_gas_cost =
        gas_costs.syscalls.get_syscall_gas_cost(&SyscallSelector::GetBlockHash).unwrap();

    // We hit the out of gas error right before executing the syscall.
    let syscall_base_gas_cost = gas_costs.base.syscall_base_gas_cost;
    let redeposit_gas = 300;
    let syscall_required_gas = get_block_hash_gas_cost - syscall_base_gas_cost;
    let call_info = entry_point_call.clone().execute_directly(&mut state).unwrap();
    assert_eq!(
        call_info.execution,
        CallExecution {
            // 'Out of gas'
            retdata: retdata![felt!["0x4f7574206f6620676173"]],
            gas_consumed: constants::REQUIRED_GAS_GET_BLOCK_HASH_TEST
                - syscall_required_gas
                - redeposit_gas,
            failed: true,
            ..Default::default()
        }
    );
}

#[test]
fn test_total_tx_limits_less_than_max_sierra_gas() {
    assert!(
        VersionedConstants::create_for_testing().initial_gas_no_user_l2_bound().0
            <= MAX_POSSIBLE_SIERRA_GAS
    );
}

#[cfg(feature = "cairo_native")]
#[rstest::rstest]
#[case(MAX_POSSIBLE_SIERRA_GAS, MAX_POSSIBLE_SIERRA_GAS - 2681170910)]
#[case(MAX_POSSIBLE_SIERRA_GAS / 10, 81886490)]
#[case(MAX_POSSIBLE_SIERRA_GAS / 100, 8190940)]
#[case(MAX_POSSIBLE_SIERRA_GAS / 1000, 822890)]
#[case(MAX_POSSIBLE_SIERRA_GAS / 10000, 85440)]
#[case(MAX_POSSIBLE_SIERRA_GAS / 100000, 12340)]
#[case(MAX_POSSIBLE_SIERRA_GAS / 1000000, 0)]
#[case(350, 0)]
#[case(35, 0)]
#[case(0, 0)]
/// Tests that Native can handle deep recursion calls without overflowing the stack.
/// Note that the recursive function must be complicated, since the compiler might transform
/// simple recursions into loops. The tested function was manually tested with higher gas and
/// reached stack overflow.
///
/// Also, there is no need to test the VM here since it doesn't use the stack.
fn test_stack_overflow(#[case] initial_gas: u64, #[case] gas_consumed: u64) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Native));
    let mut state = test_state(&ChainInfo::create_for_testing(), BALANCE, &[(test_contract, 1)]);

    let depth = felt!(1000000_u128);
    let entry_point_call = CallEntryPoint {
        calldata: calldata![depth],
        entry_point_selector: selector_from_name("test_stack_overflow"),
        initial_gas,
        ..trivial_external_entry_point_new(test_contract)
    };
    let call_info = entry_point_call.execute_directly(&mut state).unwrap();
    assert_eq!(
        call_info.execution,
        CallExecution {
            // 'Out of gas'
            retdata: retdata![felt!["0x4f7574206f6620676173"]],
            gas_consumed,
            failed: true,
            ..Default::default()
        }
    );
}
