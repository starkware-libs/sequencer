use pretty_assertions::assert_eq;
use starknet_api::felt;
use test_case::test_case;

use super::constants::REQUIRED_GAS_CALL_CONTRACT_TEST;
use crate::abi::abi_utils::selector_from_name;
use crate::context::ChainInfo;
use crate::execution::call_info::{CallExecution, Retdata};
use crate::execution::entry_point::CallEntryPoint;
use crate::retdata;
use crate::test_utils::contracts::FeatureContract;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::{create_calldata, trivial_external_entry_point_new, CairoVersion, BALANCE};

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
            felt!(405_u16), // Calldata: address.
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
