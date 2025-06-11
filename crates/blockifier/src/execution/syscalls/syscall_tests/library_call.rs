use std::collections::{HashMap, HashSet};

use cairo_vm::types::builtin_name::BuiltinName;
use pretty_assertions::assert_eq;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::{calldata, felt, storage_key};
use test_case::test_case;

use crate::context::ChainInfo;
use crate::execution::call_info::{CallExecution, CallInfo, Retdata};
use crate::execution::entry_point::{CallEntryPoint, CallType};
use crate::execution::syscalls::syscall_tests::constants::{
    REQUIRED_GAS_LIBRARY_CALL_TEST,
    REQUIRED_GAS_STORAGE_READ_WRITE_TEST,
};
use crate::retdata;
use crate::test_utils::contracts::FeatureContract;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::{trivial_external_entry_point_new, CairoVersion, RunnableCairo1, BALANCE};
use crate::versioned_constants::VersionedConstants;

#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native; "Native"))]
#[test_case(RunnableCairo1::Casm; "VM")]
fn test_library_call(runnable_version: RunnableCairo1) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(test_contract, 1)]);

    let inner_entry_point_selector = selector_from_name("test_storage_read_write");
    let calldata = calldata![
        test_contract.get_class_hash().0, // Class hash.
        inner_entry_point_selector.0,     // Function selector.
        felt!(2_u8),                      // Calldata length.
        felt!(1219_u16),                  // Calldata: address.
        felt!(91_u8)                      // Calldata: value.
    ];

    let entry_point_call = CallEntryPoint {
        entry_point_selector: selector_from_name("test_library_call"),
        calldata,
        class_hash: Some(test_contract.get_class_hash()),
        ..trivial_external_entry_point_new(test_contract)
    };

    assert_eq!(
        entry_point_call.execute_directly(&mut state).unwrap().execution,
        CallExecution {
            retdata: retdata![felt!(91_u16)],
            gas_consumed: REQUIRED_GAS_LIBRARY_CALL_TEST,
            ..Default::default()
        }
    );
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

    assert_eq!(
        call_info.execution,
        CallExecution {
            retdata: Retdata(vec![
                // 'x != y'.
                felt!("0x7820213d2079"),
                // 'ENTRYPOINT_FAILED'.
                felt!("0x454e545259504f494e545f4641494c4544")
            ]),
            gas_consumed: 105050,
            failed: true,
            ..Default::default()
        }
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
    let main_entry_point = CallEntryPoint {
        entry_point_selector: selector_from_name("test_nested_library_call"),
        calldata: main_entry_point_calldata,
        class_hash: Some(test_class_hash),
        initial_gas: 9999292440,
        ..trivial_external_entry_point_new(test_contract)
    };
    let nested_storage_entry_point = CallEntryPoint {
        entry_point_selector: inner_entry_point_selector,
        calldata: calldata![felt!(key + 1), felt!(value + 1)],
        class_hash: Some(test_class_hash),
        code_address: None,
        call_type: CallType::Delegate,
        initial_gas: 9999084430,
        ..trivial_external_entry_point_new(test_contract)
    };
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
        initial_gas: 9999185240,
        ..trivial_external_entry_point_new(test_contract)
    };
    let storage_entry_point = CallEntryPoint {
        calldata: calldata![felt!(key), felt!(value)],
        initial_gas: 9998981630,
        ..nested_storage_entry_point
    };

    // The default VersionedConstants is used in the execute_directly call bellow.
    let tracked_resource = test_contract.get_runnable_class().tracked_resource(
        &VersionedConstants::create_for_testing().min_sierra_version_for_sierra_gas,
        None,
    );

    let nested_storage_call_info = CallInfo {
        call: nested_storage_entry_point,
        execution: CallExecution {
            retdata: retdata![felt!(value + 1)],
            gas_consumed: REQUIRED_GAS_STORAGE_READ_WRITE_TEST,
            ..CallExecution::default()
        },
        tracked_resource,
        storage_read_values: vec![felt!(value + 1)],
        accessed_storage_keys: HashSet::from([storage_key!(key + 1)]),
        builtin_counters: matches!(runnable_version, RunnableCairo1::Casm)
            .then(|| HashMap::from([(BuiltinName::range_check, 7)]))
            .unwrap_or_default(),
        ..Default::default()
    };

    let library_call_info = CallInfo {
        call: library_entry_point,
        execution: CallExecution {
            retdata: retdata![felt!(value + 1)],
            gas_consumed: REQUIRED_GAS_LIBRARY_CALL_TEST,
            ..CallExecution::default()
        },
        inner_calls: vec![nested_storage_call_info],
        tracked_resource,
        builtin_counters: matches!(runnable_version, RunnableCairo1::Casm)
            .then(|| HashMap::from([(BuiltinName::range_check, 23)]))
            .unwrap_or_default(),
        ..Default::default()
    };

    let storage_call_info = CallInfo {
        call: storage_entry_point,
        execution: CallExecution {
            retdata: retdata![felt!(value)],
            gas_consumed: REQUIRED_GAS_STORAGE_READ_WRITE_TEST,
            ..CallExecution::default()
        },
        storage_read_values: vec![felt!(value)],
        accessed_storage_keys: HashSet::from([storage_key!(key)]),
        tracked_resource,
        builtin_counters: matches!(runnable_version, RunnableCairo1::Casm)
            .then(|| HashMap::from([(BuiltinName::range_check, 7)]))
            .unwrap_or_default(),
        ..Default::default()
    };

    let main_gas_consumed = 338360;
    let expected_call_info = CallInfo {
        call: main_entry_point.clone(),
        execution: CallExecution {
            retdata: retdata![felt!(value)],
            gas_consumed: main_gas_consumed,
            ..CallExecution::default()
        },
        inner_calls: vec![library_call_info, storage_call_info],
        tracked_resource,
        builtin_counters: matches!(runnable_version, RunnableCairo1::Casm)
            .then(|| HashMap::from([(BuiltinName::range_check, 35)]))
            .unwrap_or_default(),
        ..Default::default()
    };

    assert_eq!(main_entry_point.execute_directly(&mut state).unwrap(), expected_call_info);
}
