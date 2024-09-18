use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::execution_utils::format_panic_data;
use starknet_api::{calldata, felt};
use test_case::test_case;

use crate::context::ChainInfo;
use crate::execution::entry_point::CallEntryPoint;
use crate::execution::syscalls::syscall_tests::constants::REQUIRED_GAS_STORAGE_READ_WRITE_TEST;
use crate::test_utils::contracts::FeatureContract;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::{trivial_external_entry_point_new, CairoVersion, BALANCE};

#[test_case(FeatureContract::TestContract(CairoVersion::Cairo1); "VM")]
fn test_out_of_gas(test_contract: FeatureContract) {
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(test_contract, 1)]);

    let key = felt!(1234_u16);
    let value = felt!(18_u8);
    let calldata = calldata![key, value];
    let entry_point_call = CallEntryPoint {
        calldata,
        entry_point_selector: selector_from_name("test_storage_read_write"),
        initial_gas: REQUIRED_GAS_STORAGE_READ_WRITE_TEST - 1,
        ..trivial_external_entry_point_new(test_contract)
    };
    let call_info = entry_point_call.execute_directly(&mut state).unwrap();
    assert!(call_info.execution.failed);
    assert_eq!(
        format_panic_data(&call_info.execution.retdata.0),
        "0x4f7574206f6620676173 ('Out of gas')"
    );
}
