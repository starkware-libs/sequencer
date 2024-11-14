use std::collections::HashSet;

use pretty_assertions::assert_eq;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::execution_resources::GasAmount;
use starknet_api::transaction::fields::GasVectorComputationMode;
use starknet_api::{calldata, felt, storage_key};
use test_case::test_case;

use crate::context::ChainInfo;
use crate::execution::call_info::{CallExecution, CallInfo, ChargedResources, Retdata};
use crate::execution::entry_point::{CallEntryPoint, CallType};
use crate::execution::syscalls::syscall_tests::constants::{
    REQUIRED_GAS_LIBRARY_CALL_TEST,
    REQUIRED_GAS_STORAGE_READ_WRITE_TEST,
};
use crate::retdata;
use crate::test_utils::contracts::FeatureContract;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::{trivial_external_entry_point_new, CairoVersion, BALANCE};
use crate::versioned_constants::VersionedConstants;

#[cfg_attr(
    feature = "cairo_native",
    test_case(FeatureContract::TestContract(CairoVersion::Native); "Native")
)]
#[test_case(FeatureContract::TestContract(CairoVersion::Cairo1); "VM")]
fn test_library_call(test_contract: FeatureContract) {
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
    let expected_err_retdata = match test_contract.cairo_version() {
        CairoVersion::Cairo0 | CairoVersion::Cairo1 => {
            // 'x != y', 'ENTRYPOINT_FAILED'.
            vec![felt!("0x7820213d2079"), felt!("0x454e545259504f494e545f4641494c4544")]
        }
        #[cfg(feature = "cairo_native")]
        // 'x != y'.
        CairoVersion::Native => vec![felt!("0x7820213d2079")],
    };

    assert_eq!(
        call_info.execution,
        CallExecution {
            retdata: Retdata(expected_err_retdata),
            gas_consumed: 150980,
            failed: true,
            ..Default::default()
        }
    );
}

#[cfg_attr(
    feature = "cairo_native",
    test_case(FeatureContract::TestContract(CairoVersion::Native); "Native")
)]
#[test_case(FeatureContract::TestContract(CairoVersion::Cairo1); "VM")]
fn test_nested_library_call(test_contract: FeatureContract) {
    // Todo(pwhite) 2024/10/28: Execution resources from the VM & Native are mesaured differently
    // helper function to change the expected resource values from both of executions
    // When gas is changed to be the same between VM and Native this should be removed.
    #[cfg_attr(not(feature = "cairo_native"), allow(unused_variables))]
    fn if_native<T>(test_contract: &FeatureContract) -> impl Fn(T, T) -> T + '_ {
        move |native: T, non_native: T| {
            #[cfg(feature = "cairo_native")]
            {
                if matches!(test_contract, FeatureContract::TestContract(CairoVersion::Native)) {
                    native
                } else {
                    non_native
                }
            }
            #[cfg(not(feature = "cairo_native"))]
            {
                non_native
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
        initial_gas: 9999292440,
        ..trivial_external_entry_point_new(test_contract)
    };
    let nested_storage_entry_point = CallEntryPoint {
        entry_point_selector: inner_entry_point_selector,
        calldata: calldata![felt!(key + 1), felt!(value + 1)],
        class_hash: Some(test_class_hash),
        code_address: None,
        call_type: CallType::Delegate,
        initial_gas: 9998985960,
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
        initial_gas: 9999136940,
        ..trivial_external_entry_point_new(test_contract)
    };
    let storage_entry_point = CallEntryPoint {
        calldata: calldata![felt!(key), felt!(value)],
        initial_gas: 9998834320,
        ..nested_storage_entry_point
    };

    let storage_entry_point_gas = GasAmount(if_native(&test_contract)(26990, 16990));

    // The default VersionedConstants is used in the execute_directly call bellow.
    let tracked_resource = test_contract.get_runnable_class().tracked_resource(
        &VersionedConstants::create_for_testing().min_compiler_version_for_sierra_gas,
        GasVectorComputationMode::All,
    );

    let nested_storage_call_info = CallInfo {
        call: nested_storage_entry_point,
        execution: CallExecution {
            retdata: retdata![felt!(value + 1)],
            gas_consumed: REQUIRED_GAS_STORAGE_READ_WRITE_TEST,
            ..CallExecution::default()
        },
        charged_resources: ChargedResources::from_gas(storage_entry_point_gas),
        tracked_resource,
        storage_read_values: vec![felt!(value + 1)],
        accessed_storage_keys: HashSet::from([storage_key!(key + 1)]),
        ..Default::default()
    };

    let library_call_info = CallInfo {
        call: library_entry_point,
        execution: CallExecution {
            retdata: retdata![felt!(value + 1)],
            gas_consumed: REQUIRED_GAS_LIBRARY_CALL_TEST,
            ..CallExecution::default()
        },
        charged_resources: ChargedResources::from_gas(GasAmount(if_native(&test_contract)(
            187970, 167970,
        ))),
        inner_calls: vec![nested_storage_call_info],
        tracked_resource,
        ..Default::default()
    };

    let storage_call_info = CallInfo {
        call: storage_entry_point,
        execution: CallExecution {
            retdata: retdata![felt!(value)],
            gas_consumed: REQUIRED_GAS_STORAGE_READ_WRITE_TEST,
            ..CallExecution::default()
        },
        charged_resources: ChargedResources::from_gas(storage_entry_point_gas),
        storage_read_values: vec![felt!(value)],
        accessed_storage_keys: HashSet::from([storage_key!(key)]),
        tracked_resource,
        ..Default::default()
    };

    let expected_call_info = CallInfo {
        call: main_entry_point.clone(),
        execution: CallExecution {
            retdata: retdata![felt!(value)],
            gas_consumed: 475110,
            ..CallExecution::default()
        },
        charged_resources: ChargedResources::from_gas(GasAmount(if_native(&test_contract)(
            515110, 475110,
        ))),
        inner_calls: vec![library_call_info, storage_call_info],
        tracked_resource,
        ..Default::default()
    };

    assert_eq!(main_entry_point.execute_directly(&mut state).unwrap(), expected_call_info);
}
