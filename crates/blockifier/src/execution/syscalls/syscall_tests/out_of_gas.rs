use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::{calldata, felt};
use test_case::test_case;

use crate::abi::constants::MAX_POSSIBLE_SIERRA_GAS;
use crate::blockifier_versioned_constants::VersionedConstants;
use crate::execution::call_info::CallExecution;
use crate::execution::entry_point::CallEntryPoint;
use crate::execution::syscalls::syscall_tests::constants;
use crate::execution::syscalls::syscall_tests::get_block_hash::initialize_state;
use crate::execution::syscalls::vm_syscall_utils::SyscallSelector;
use crate::retdata;
use crate::test_utils::trivial_external_entry_point_new;

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
    let syscall_required_gas = get_block_hash_gas_cost.base_syscall_cost() - syscall_base_gas_cost;
    let call_info = entry_point_call.clone().execute_directly(&mut state).unwrap();
    assert_eq!(
        call_info.execution,
        CallExecution {
            // 'Out of gas'
            retdata: retdata![felt!["0x4f7574206f6620676173"]],
            gas_consumed: constants::REQUIRED_GAS_GET_BLOCK_HASH_TEST - syscall_required_gas,
            cairo_native: runnable_version.is_cairo_native(),
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
