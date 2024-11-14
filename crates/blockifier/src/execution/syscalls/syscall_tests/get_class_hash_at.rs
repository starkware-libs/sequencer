use starknet_api::{calldata, class_hash, contract_address, felt};
use test_case::test_case;

use crate::abi::abi_utils::selector_from_name;
use crate::context::ChainInfo;
use crate::execution::call_info::CallExecution;
use crate::execution::entry_point::CallEntryPoint;
use crate::execution::syscalls::syscall_tests::constants::REQUIRED_GAS_GET_CLASS_HASH_AT_TEST;
use crate::retdata;
use crate::test_utils::contracts::FeatureContract;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::{trivial_external_entry_point_new, CairoVersion, BALANCE};

/// Tests the `get_class_hash_at` syscall, ensuring that:
/// 1. `accessed_contract_addresses` contains `address` for a valid entry.
/// 2. `read_class_hash_values` includes `class_hash`.
/// 3. Execution succeeds with expected gas for valid cases.
/// 4. Execution fails if `address` has a different `class_hash`.
/// 5. Execution succeeds and returns `class_hash` = 0 if `address` is absent.
#[test_case(FeatureContract::TestContract(CairoVersion::Cairo1), REQUIRED_GAS_GET_CLASS_HASH_AT_TEST; "VM")]
#[cfg_attr(
    feature = "cairo_native",
    test_case(FeatureContract::TestContract(CairoVersion::Native), 17830; "Native"))
]
fn test_get_class_hash_at(test_contract: FeatureContract, expected_gas: u64) {
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(test_contract, 1)]);
    let address = contract_address!("0x111");
    let class_hash = class_hash!("0x222");
    state.state.address_to_class_hash.insert(address, class_hash);

    // Test deployed contract.
    let positive_entry_point_call = CallEntryPoint {
        calldata: calldata![address.into(), class_hash.0],
        entry_point_selector: selector_from_name("test_get_class_hash_at"),
        ..trivial_external_entry_point_new(test_contract)
    };
    let positive_call_info = positive_entry_point_call.execute_directly(&mut state).unwrap();
    assert!(positive_call_info.accessed_contract_addresses.contains(&address));
    assert!(positive_call_info.read_class_hash_values[0] == class_hash);
    assert_eq!(
        positive_call_info.execution,
        CallExecution {
            retdata: retdata!(),
            gas_consumed: expected_gas,
            failed: false,
            ..CallExecution::default()
        }
    );
    // Test undeployed contract - should return class_hash = 0 and succeed.
    let non_existing_address = felt!("0x333");
    let class_hash_of_undeployed_contract = felt!("0x0");

    let negative_entry_point_call = CallEntryPoint {
        calldata: calldata![non_existing_address, class_hash_of_undeployed_contract],
        entry_point_selector: selector_from_name("test_get_class_hash_at"),
        ..trivial_external_entry_point_new(test_contract)
    };
    assert!(!negative_entry_point_call.execute_directly(&mut state).unwrap().execution.failed);

    // Sanity check: giving the wrong expected class hash to the test should make it fail.
    let different_class_hash = class_hash!("0x444");
    let different_class_hash_entry_point_call = CallEntryPoint {
        calldata: calldata![address.into(), different_class_hash.0],
        entry_point_selector: selector_from_name("test_get_class_hash_at"),
        ..trivial_external_entry_point_new(test_contract)
    };
    assert!(
        different_class_hash_entry_point_call
            .execute_directly(&mut state)
            .unwrap()
            .execution
            .failed
    );
}
