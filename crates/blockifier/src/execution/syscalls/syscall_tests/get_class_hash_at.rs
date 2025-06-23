use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use expect_test::expect;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::{calldata, class_hash, contract_address, felt};
use test_case::test_case;

use crate::context::ChainInfo;
use crate::execution::entry_point::CallEntryPoint;
use crate::retdata;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::{trivial_external_entry_point_new, BALANCE};

/// Tests the `get_class_hash_at` syscall, ensuring that:
/// 1. `accessed_contract_addresses` contains `address` for a valid entry.
/// 2. `read_class_hash_values` includes `class_hash`.
/// 3. Execution succeeds with expected gas for valid cases.
/// 4. Execution fails if `address` has a different `class_hash`.
/// 5. Execution succeeds and returns `class_hash` = 0 if `address` is absent.
#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native;"Native"))]
#[test_case(RunnableCairo1::Casm;"VM")]
fn test_get_class_hash_at(runnable_version: RunnableCairo1) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
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
    let positive_call_info =
        positive_entry_point_call.clone().execute_directly(&mut state).unwrap();
    assert!(
        positive_call_info.storage_access_tracker.accessed_contract_addresses.contains(&address)
    );
    assert!(positive_call_info.storage_access_tracker.read_class_hash_values[0] == class_hash);
    expect![[r#"
        CallExecution {
            retdata: Retdata(
                [],
            ),
            events: [],
            l2_to_l1_messages: [],
            failed: false,
            gas_consumed: 16460,
            cairo_native: runnable_version.is_cairo_native(),
        }
    "#]]
    .assert_debug_eq(&positive_call_info.execution);
    assert!(!positive_call_info.execution.failed);
    assert_eq!(positive_call_info.execution.retdata, retdata![]);
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

    let error =
        positive_entry_point_call.execute_directly_in_validate_mode(&mut state).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Unauthorized syscall get_class_hash_at in execution mode Validate.")
    );
}
