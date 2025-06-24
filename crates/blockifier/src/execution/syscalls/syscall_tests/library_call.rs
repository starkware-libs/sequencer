use std::collections::{HashMap, HashSet};

use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use cairo_vm::types::builtin_name::BuiltinName;
use expect_test::expect;
use pretty_assertions::assert_eq;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::{calldata, felt, storage_key};
use test_case::test_case;

use crate::blockifier_versioned_constants::VersionedConstants;
use crate::context::ChainInfo;
use crate::execution::call_info::{CallExecution, CallInfo, Retdata, StorageAccessTracker};
use crate::execution::entry_point::{CallEntryPoint, CallType};
use crate::retdata;
use crate::test_utils::contracts::FeatureContractTrait;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::{trivial_external_entry_point_new, BALANCE};

#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native; "Native"))]
#[test_case(RunnableCairo1::Casm; "VM")]
fn test_library_call(runnable_version: RunnableCairo1) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(test_contract, 1)]);
    let value = felt!(91_u8);

    let inner_entry_point_selector = selector_from_name("test_storage_read_write");
    let calldata = calldata![
        test_contract.get_class_hash().0, // Class hash.
        inner_entry_point_selector.0,     // Function selector.
        felt!(2_u8),                      // Calldata length.
        felt!(1219_u16),                  // Calldata: address.
        value                             // Calldata: value.
    ];

    let entry_point_call = CallEntryPoint {
        entry_point_selector: selector_from_name("test_library_call"),
        calldata,
        class_hash: Some(test_contract.get_class_hash()),
        ..trivial_external_entry_point_new(test_contract)
    };

    let execution = entry_point_call.execute_directly(&mut state).unwrap().execution;
    if runnable_version.is_cairo_native() {
        expect![[r#"
        CallExecution {
            retdata: Retdata(
                [
                    0x5b,
                ],
            ),
            events: [],
            l2_to_l1_messages: [],
            cairo_native: true,
            failed: false,
            gas_consumed: 127470,
        }
    "#]]
    } else {
        expect![[r#"
        CallExecution {
            retdata: Retdata(
                [
                    0x5b,
                ],
            ),
            events: [],
            l2_to_l1_messages: [],
            cairo_native: false,
            failed: false,
            gas_consumed: 127470,
        }
    "#]]
    }
    .assert_debug_eq(&execution);
    assert_eq!(execution.retdata, retdata![value]);
}

#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native; "Native"))]
#[test_case(RunnableCairo1::Casm; "VM")]
fn test_library_call_assert_fails(runnable_version: RunnableCairo1) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(test_contract, 1)]);
    let inner_entry_point_selector = selector_from_name("assert_eq");
    let calldata = calldata![
        test_contract.get_class_hash().0, // Class hash.
        inner_entry_point_selector.0,     // Function selector.
        felt!(2_u8),                      // Calldata length.
        felt!(0_u8),                      // Calldata: first assert value.
        felt!(1_u8)                       // Calldata: second assert value.
    ];

    let entry_point_call = CallEntryPoint {
        entry_point_selector: selector_from_name("test_library_call"),
        calldata,
        class_hash: Some(test_contract.get_class_hash()),
        ..trivial_external_entry_point_new(test_contract)
    };
    let call_info = entry_point_call.execute_directly(&mut state).unwrap();

    // TODO(Meshi): refactor so there is no need for the if else.
    if runnable_version.is_cairo_native() {
        expect![[r#"
        CallExecution {
            retdata: Retdata(
                [
                    0x7820213d2079,
                    0x454e545259504f494e545f4641494c4544,
                ],
            ),
            events: [],
            l2_to_l1_messages: [],
            cairo_native: true,
            failed: true,
            gas_consumed: 111020,
        }
    "#]]
    } else {
        expect![[r#"
        CallExecution {
            retdata: Retdata(
                [
                    0x7820213d2079,
                    0x454e545259504f494e545f4641494c4544,
                ],
            ),
            events: [],
            l2_to_l1_messages: [],
            cairo_native: false,
            failed: true,
            gas_consumed: 111020,
        }
    "#]]
    }
    .assert_debug_eq(&call_info.execution);
    assert!(call_info.execution.failed);
    assert_eq!(
        call_info.execution.retdata,
        Retdata(vec![
            // 'x != y'.
            felt!("0x7820213d2079"),
            // 'ENTRYPOINT_FAILED'.
            felt!("0x454e545259504f494e545f4641494c4544")
        ])
    );
}

#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native; "Native"))]
#[test_case(RunnableCairo1::Casm; "VM")]
fn test_nested_library_call(runnable_version: RunnableCairo1) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(test_contract, 1)]);

    let (key, value) = (255_u64, 44_u64);
    let outer_entry_point_selector = selector_from_name("test_library_call");
    let inner_entry_point_selector = selector_from_name("test_storage_read_write");
    let test_class_hash = test_contract.get_class_hash();
    let main_entry_point_calldata = calldata![
        test_class_hash.0,            // Class hash.
        outer_entry_point_selector.0, // Library call function selector.
        inner_entry_point_selector.0, // Storage function selector.
        felt!(key),                   // Calldata: address.
        felt!(value)                  // Calldata: value.
    ];

    // Create expected call info tree.
    let main_initial_gas = 9999292440;
    let main_entry_point = CallEntryPoint {
        entry_point_selector: selector_from_name("test_nested_library_call"),
        calldata: main_entry_point_calldata,
        class_hash: Some(test_class_hash),
        initial_gas: main_initial_gas,
        ..trivial_external_entry_point_new(test_contract)
    };
    let expected_nested_initial_gas = expect![[r#"
        9999081600
    "#]];
    let nested_storage_entry_point = CallEntryPoint {
        entry_point_selector: inner_entry_point_selector,
        calldata: calldata![felt!(key + 1), felt!(value + 1)],
        class_hash: Some(test_class_hash),
        code_address: None,
        call_type: CallType::Delegate,
        initial_gas: 0, // Tested via expect![] macro.
        ..trivial_external_entry_point_new(test_contract)
    };
    let expected_library_initial_gas = expect![[r#"
        9999182620
    "#]];
    let library_entry_point = CallEntryPoint {
        entry_point_selector: outer_entry_point_selector,
        calldata: calldata![
            test_class_hash.0,            // Class hash.
            inner_entry_point_selector.0, // Storage function selector.
            felt!(2_u8),                  // Calldata: address.
            felt!(key + 1),               // Calldata: address.
            felt!(value + 1)              // Calldata: value.
        ],
        class_hash: Some(test_class_hash),
        code_address: None,
        call_type: CallType::Delegate,
        initial_gas: 0, // Tested via expect![] macro.
        ..trivial_external_entry_point_new(test_contract)
    };
    let expected_storage_initial_gas = expect![[r#"
        9998975990
    "#]];
    let storage_entry_point = CallEntryPoint {
        calldata: calldata![felt!(key), felt!(value)],
        initial_gas: 0, // Tested via expect![] macro.
        ..nested_storage_entry_point
    };

    // The default VersionedConstants is used in the execute_directly call bellow.
    let tracked_resource = test_contract.get_runnable_class().tracked_resource(
        &VersionedConstants::create_for_testing().min_sierra_version_for_sierra_gas,
        None,
    );

    let expected_nested_gas_consumed = expect![[r#"
        26450
    "#]];
    let nested_storage_call_info = CallInfo {
        call: nested_storage_entry_point,
        execution: CallExecution {
            retdata: retdata![felt!(value + 1)],
            gas_consumed: 0, // Tested via expect![] macro.
            cairo_native: runnable_version.is_cairo_native(),
            ..CallExecution::default()
        },
        tracked_resource,
        storage_access_tracker: StorageAccessTracker {
            storage_read_values: vec![felt!(value + 1)],
            accessed_storage_keys: HashSet::from([storage_key!(key + 1)]),
            ..Default::default()
        },
        builtin_counters: matches!(runnable_version, RunnableCairo1::Casm)
            .then(|| HashMap::from([(BuiltinName::range_check, 7)]))
            .unwrap_or_default(),
        ..Default::default()
    };

    let expected_library_call_gas_consumed = expect![[r#"
        127470
    "#]];
    let library_call_info = CallInfo {
        call: library_entry_point,
        execution: CallExecution {
            retdata: retdata![felt!(value + 1)],
            gas_consumed: 0, // Tested via expect![] macro.
            cairo_native: runnable_version.is_cairo_native(),
            ..CallExecution::default()
        },
        inner_calls: vec![nested_storage_call_info],
        tracked_resource,
        builtin_counters: matches!(runnable_version, RunnableCairo1::Casm)
            .then(|| HashMap::from([(BuiltinName::range_check, 26)]))
            .unwrap_or_default(),
        ..Default::default()
    };

    let expected_storage_call_gas_consumed = expect![[r#"
        26450
    "#]];
    let storage_call_info = CallInfo {
        call: storage_entry_point,
        execution: CallExecution {
            retdata: retdata![felt!(value)],
            gas_consumed: 0, // Tested via expect![] macro.
            cairo_native: runnable_version.is_cairo_native(),
            ..CallExecution::default()
        },
        storage_access_tracker: StorageAccessTracker {
            storage_read_values: vec![felt!(value)],
            accessed_storage_keys: HashSet::from([storage_key!(key)]),
            ..Default::default()
        },
        tracked_resource,
        builtin_counters: matches!(runnable_version, RunnableCairo1::Casm)
            .then(|| HashMap::from([(BuiltinName::range_check, 7)]))
            .unwrap_or_default(),
        ..Default::default()
    };

    let expected_main_gas_consumed = expect![[r#"
        342890
    "#]];
    let expected_call_info = CallInfo {
        call: main_entry_point.clone(),
        execution: CallExecution {
            retdata: retdata![felt!(value)],
            gas_consumed: 0, // Tested via expect![] macro.
            cairo_native: runnable_version.is_cairo_native(),
            ..CallExecution::default()
        },
        inner_calls: vec![library_call_info, storage_call_info],
        tracked_resource,
        builtin_counters: matches!(runnable_version, RunnableCairo1::Casm)
            .then(|| HashMap::from([(BuiltinName::range_check, 41)]))
            .unwrap_or_default(),
        ..Default::default()
    };

    let mut result = main_entry_point.execute_directly(&mut state).unwrap();

    // Regression-test specific values and set to zero for comparison.
    let [library_call, storage_call] = &mut result.inner_calls[..] else {
        panic!("Expected 2 inner calls, got {}", result.inner_calls.len());
    };
    let nested_call = &mut library_call.inner_calls[0];
    expected_nested_gas_consumed.assert_debug_eq(&nested_call.execution.gas_consumed);
    expected_nested_initial_gas.assert_debug_eq(&nested_call.call.initial_gas);
    expected_library_call_gas_consumed.assert_debug_eq(&library_call.execution.gas_consumed);
    expected_library_initial_gas.assert_debug_eq(&library_call.call.initial_gas);
    expected_storage_call_gas_consumed.assert_debug_eq(&storage_call.execution.gas_consumed);
    expected_storage_initial_gas.assert_debug_eq(&storage_call.call.initial_gas);
    expected_main_gas_consumed.assert_debug_eq(&result.execution.gas_consumed);
    nested_call.execution.gas_consumed = 0;
    nested_call.call.initial_gas = 0;
    library_call.execution.gas_consumed = 0;
    library_call.call.initial_gas = 0;
    storage_call.execution.gas_consumed = 0;
    storage_call.call.initial_gas = 0;
    result.execution.gas_consumed = 0;

    assert_eq!(result, expected_call_info);
}
