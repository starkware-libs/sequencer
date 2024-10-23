use std::collections::{HashMap, HashSet};

use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use pretty_assertions::assert_eq;
use starknet_api::execution_utils::format_panic_data;
use starknet_api::transaction::GasVectorComputationMode;
use starknet_api::{calldata, felt, storage_key};
use test_case::test_case;

use crate::abi::abi_utils::selector_from_name;
use crate::context::ChainInfo;
use crate::execution::call_info::{CallExecution, CallInfo, ChargedResources};
use crate::execution::entry_point::{CallEntryPoint, CallType};
use crate::execution::syscalls::syscall_tests::constants::{
    REQUIRED_GAS_LIBRARY_CALL_TEST,
    REQUIRED_GAS_STORAGE_READ_WRITE_TEST,
};
use crate::execution::syscalls::SyscallSelector;
use crate::retdata;
use crate::test_utils::contracts::FeatureContract;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::{
    get_syscall_resources,
    trivial_external_entry_point_new,
    CairoVersion,
    BALANCE,
};
use crate::versioned_constants::VersionedConstants;

#[cfg_attr(
    feature = "cairo_native",
    test_case(FeatureContract::TestContract(CairoVersion::Native), 189470; "Native")
)]
#[test_case(FeatureContract::TestContract(CairoVersion::Cairo1), REQUIRED_GAS_LIBRARY_CALL_TEST; "VM")]
fn test_library_call(test_contract: FeatureContract, expected_gas: u64) {
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(test_contract, 1)]);

    let inner_entry_point_selector = selector_from_name("test_storage_read_write");
    let calldata = calldata![
        test_contract.get_class_hash().0, // Class hash.
        inner_entry_point_selector.0,     // Function selector.
        felt!(2_u8),                      // Calldata length.
        felt!(1234_u16),                  // Calldata: address.
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
            gas_consumed: expected_gas,
            ..Default::default()
        }
    );
}

#[cfg_attr(
    feature = "cairo_native",
    test_case(FeatureContract::TestContract(CairoVersion::Native); "Native")
)]
#[test_case(FeatureContract::TestContract(CairoVersion::Cairo1); "VM")]
fn test_library_call_assert_fails(test_contract: FeatureContract) {
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
    assert!(call_info.execution.failed);

    let expected_err = match test_contract.cairo_version() {
        CairoVersion::Cairo0 | CairoVersion::Cairo1 => {
            "(0x7820213d2079 ('x != y'), 0x454e545259504f494e545f4641494c4544 \
             ('ENTRYPOINT_FAILED'))"
        }
        #[cfg(feature = "cairo_native")]
        CairoVersion::Native => "0x7820213d2079 ('x != y')",
    };
    assert_eq!(format_panic_data(&call_info.execution.retdata.0), expected_err);
}

#[cfg_attr(
    feature = "cairo_native",
    test_case(FeatureContract::TestContract(CairoVersion::Native), 518110; "Native")
)]
#[test_case(FeatureContract::TestContract(CairoVersion::Cairo1), 478110; "VM")]
fn test_nested_library_call(test_contract: FeatureContract, expected_gas: u64) {
    // Todo(pwhite) 2024/10/28: Execution resources from the VM & Native are mesaured differently
    // helper function to change the expected resource values from both of executions
    // When gas is changed to be the same between VM and Native this should be removed.
    #[cfg_attr(not(feature = "cairo_native"), allow(unused_variables))]
    fn if_native<T>(test_contract: &FeatureContract) -> impl Fn(T, T) -> T + '_ {
        move |native_value: T, non_native_value: T| {
            #[cfg(feature = "cairo_native")]
            {
                if matches!(test_contract, FeatureContract::TestContract(CairoVersion::Native)) {
                    native_value
                } else {
                    non_native_value
                }
            }
            #[cfg(not(feature = "cairo_native"))]
            {
                non_native_value
            }
        }
    }

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
        initial_gas: 9999906600,
        ..trivial_external_entry_point_new(test_contract)
    };
    let nested_storage_entry_point = CallEntryPoint {
        entry_point_selector: inner_entry_point_selector,
        calldata: calldata![felt!(key + 1), felt!(value + 1)],
        class_hash: Some(test_class_hash),
        code_address: None,
        call_type: CallType::Delegate,
        initial_gas: if_native(&test_contract)(9999577720, 9999597720),
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
        initial_gas: if_native(&test_contract)(9999739900, 9999749900),
        ..trivial_external_entry_point_new(test_contract)
    };
    let storage_entry_point = CallEntryPoint {
        calldata: calldata![felt!(key), felt!(value)],
        initial_gas: if_native(&test_contract)(9999415780, 9999445780),
        ..nested_storage_entry_point
    };

    let first_storage_entry_point_resources = if_native(&test_contract)(
        ExecutionResources {
            n_steps: 180,
            n_memory_holes: 0,
            builtin_instance_counter: HashMap::from([(BuiltinName::range_check, 2)]),
        },
        ExecutionResources {
            n_steps: 247,
            n_memory_holes: 0,
            builtin_instance_counter: HashMap::from([(BuiltinName::range_check, 7)]),
        },
    );
    let storage_entry_point_resources = if_native(&test_contract)(
        ExecutionResources {
            n_steps: 1202,
            n_memory_holes: 0,
            builtin_instance_counter: HashMap::from([(BuiltinName::range_check, 19)]),
        },
        first_storage_entry_point_resources.clone(),
    );

    // The default VersionedConstants is used in the execute_directly call bellow.
    let tracked_resource = test_contract.get_runnable_class().tracked_resource(
        &VersionedConstants::create_for_testing().min_compiler_version_for_sierra_gas,
        GasVectorComputationMode::All,
    );

    let nested_storage_call_info = CallInfo {
        call: nested_storage_entry_point,
        execution: CallExecution {
            retdata: retdata![felt!(value + 1)],
            gas_consumed: if_native(&test_contract)(27290, REQUIRED_GAS_STORAGE_READ_WRITE_TEST),
            ..CallExecution::default()
        },
        charged_resources: ChargedResources::from_execution_resources(if_native(&test_contract)(
            ExecutionResources {
                n_steps: 180,
                n_memory_holes: 0,
                builtin_instance_counter: HashMap::from([(BuiltinName::range_check, 2)]),
            },
            storage_entry_point_resources.clone(),
        )),
        tracked_resource,
        storage_read_values: vec![felt!(value + 1)],
        accessed_storage_keys: HashSet::from([storage_key!(key + 1)]),
        ..Default::default()
    };

    let library_call_resources = if_native(&test_contract)(
        ExecutionResources {
            n_steps: 1022,
            n_memory_holes: 0,
            builtin_instance_counter: HashMap::from([(BuiltinName::range_check, 17)]),
        },
        &get_syscall_resources(SyscallSelector::LibraryCall)
            + &ExecutionResources {
                n_steps: 392,
                n_memory_holes: 0,
                builtin_instance_counter: HashMap::from([(BuiltinName::range_check, 15)]),
            },
    );

    let library_call_info = CallInfo {
        call: library_entry_point,
        execution: CallExecution {
            retdata: retdata![felt!(value + 1)],
            gas_consumed: if_native(&test_contract)(189470, REQUIRED_GAS_LIBRARY_CALL_TEST),
            ..CallExecution::default()
        },
        charged_resources: ChargedResources::from_execution_resources(library_call_resources),
        inner_calls: vec![nested_storage_call_info],
        tracked_resource,
        ..Default::default()
    };

    let storage_call_info = CallInfo {
        call: storage_entry_point,
        execution: CallExecution {
            retdata: retdata![felt!(value)],
            gas_consumed: if_native(&test_contract)(27290, REQUIRED_GAS_STORAGE_READ_WRITE_TEST),
            ..CallExecution::default()
        },
        charged_resources: ChargedResources::from_execution_resources(
            storage_entry_point_resources,
        ),
        storage_read_values: vec![felt!(value)],
        accessed_storage_keys: HashSet::from([storage_key!(key)]),
        tracked_resource,
        ..Default::default()
    };

    let main_call_resources = if_native(&test_contract)(
        ExecutionResources {
            n_steps: 2886,
            n_memory_holes: 0,
            builtin_instance_counter: HashMap::from([(BuiltinName::range_check, 49)]),
        },
        &(&get_syscall_resources(SyscallSelector::LibraryCall) * 3)
            + &ExecutionResources {
                n_steps: 757,
                n_memory_holes: 2,
                builtin_instance_counter: HashMap::from([(BuiltinName::range_check, 27)]),
            },
    );

    let expected_call_info = CallInfo {
        call: main_entry_point.clone(),
        execution: CallExecution {
            retdata: retdata![felt!(value)],
            gas_consumed: expected_gas,
            ..CallExecution::default()
        },
        charged_resources: ChargedResources::from_execution_resources(main_call_resources),
        inner_calls: vec![library_call_info, storage_call_info],
        tracked_resource,
        ..Default::default()
    };

    assert_eq!(main_entry_point.execute_directly(&mut state).unwrap(), expected_call_info);
}
