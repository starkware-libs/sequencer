use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::{calldata, felt};
use test_case::test_case;

use crate::context::ChainInfo;
use crate::execution::call_info::CallExecution;
use crate::execution::entry_point::CallEntryPoint;
use crate::execution::syscalls::syscall_tests::constants::REQUIRED_GAS_STORAGE_READ_WRITE_TEST;
use crate::retdata;
use crate::test_utils::contracts::{FeatureContract, RunnableContractVersion};
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::{trivial_external_entry_point_new, BALANCE};

#[cfg_attr(
    feature = "cairo_native",
    test_case(RunnableContractVersion::Cairo1Native; "Native")
)]
#[test_case(RunnableContractVersion::Cairo1Casm;"VM")]
fn test_out_of_gas(cairo_version: RunnableContractVersion) {
    let test_contract = FeatureContract::TestContract(cairo_version);
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
    assert_eq!(
        call_info.execution,
        CallExecution {
            // 'Out of gas'
            retdata: retdata![felt!["0x4f7574206f6620676173"]],
            gas_consumed: REQUIRED_GAS_STORAGE_READ_WRITE_TEST - 70,
            failed: true,
            ..Default::default()
        }
    );
}
