use std::collections::HashMap;

use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use cairo_vm::types::builtin_name::BuiltinName;
use pretty_assertions::assert_eq;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::contract_class::SierraVersion;
use starknet_api::core::{calculate_contract_address, ContractAddress};
use starknet_api::transaction::fields::{Calldata, ContractAddressSalt, Fee};
use starknet_api::felt;
use starknet_types_core::felt::Felt;
use test_case::test_case;

use crate::context::{BlockContext, ChainInfo};
use crate::execution::call_info::CallExecution;
use crate::execution::entry_point::CallEntryPoint;
use crate::execution::syscalls::hint_processor::SyscallUsage;
use crate::execution::syscalls::SyscallSelector;
use crate::retdata;
use crate::state::cached_state::CachedState;
use crate::state::state_api::StateReader;
use crate::test_utils::dict_state_reader::DictStateReader;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::{calldata_for_deploy_test, trivial_external_entry_point_new};

fn setup_deploy_call(
    contract_instances: &[(FeatureContract, u16)],
    constructor_calldata: Vec<Felt>,
    deploy_from_zero: bool,
) -> (CallEntryPoint, ContractAddress, CachedState<DictStateReader>) {
    let class_hash = contract_instances.last().unwrap().0.get_class_hash();
    let calldata = calldata_for_deploy_test(class_hash, &constructor_calldata, deploy_from_zero);

    let state = test_state(&ChainInfo::create_for_testing(), Fee(0), contract_instances);
    let deployer_contract = contract_instances[0].0;

    let contract_address = calculate_contract_address(
        ContractAddressSalt::default(),
        class_hash,
        &Calldata(constructor_calldata.clone().into()),
        deployer_contract.get_instance_address(0),
    )
    .unwrap();

    let entry_point_call = CallEntryPoint {
        entry_point_selector: selector_from_name("test_deploy"),
        calldata,
        ..trivial_external_entry_point_new(deployer_contract)
    };

    (entry_point_call, contract_address, state)
}

#[test_case(RunnableCairo1::Casm;"VM")]
#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native;"Native"))]
fn no_constructor(runnable_version: RunnableCairo1) {
    let deployer_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let empty_contract = FeatureContract::Empty(CairoVersion::Cairo1(runnable_version));

    let (entry_point, deployed_contract_address, mut state) =
        setup_deploy_call(&[(deployer_contract, 1), (empty_contract, 0)], vec![], true);

    let deploy_call = &entry_point.execute_directly(&mut state).unwrap();

    let class_hash = empty_contract.get_class_hash();

    assert_eq!(
        deploy_call.execution,
        CallExecution { retdata: retdata![], gas_consumed: 158600, ..CallExecution::default() }
    );

    let constructor_call = &deploy_call.inner_calls[0];

    assert_eq!(constructor_call.call.storage_address, deployed_contract_address);
    assert_eq!(
        constructor_call.execution,
        CallExecution { retdata: retdata![], gas_consumed: 0, ..CallExecution::default() }
    );
    assert_eq!(state.get_class_hash_at(deployed_contract_address).unwrap(), class_hash);
}

#[test_case(RunnableCairo1::Casm;"VM")]
#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native;"Native"))]
fn no_constructor_nonempty_calldata(runnable_version: RunnableCairo1) {
    let deployer_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let empty_contract = FeatureContract::Empty(CairoVersion::Cairo1(runnable_version));

    let (entry_point, _deployed_addr, mut state) = setup_deploy_call(
        &[(deployer_contract, 1), (empty_contract, 0)],
        vec![felt!(1_u8), felt!(1_u8)],
        true,
    );

    let error = entry_point.execute_directly(&mut state).unwrap_err().to_string();
    assert!(error.contains(
        "Invalid input: constructor_calldata; Cannot pass calldata to a contract with no \
         constructor."
    ));
}

#[test_case(RunnableCairo1::Casm;"VM")]
#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native;"Native"))]
fn with_constructor(runnable_version: RunnableCairo1) {
    let deployer_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let constructor_calldata = vec![
        felt!(1_u8), // Calldata: address.
        felt!(1_u8), // Calldata: value.
    ];
    let (entry_point_call, _deployed_addr, mut state) =
        setup_deploy_call(&[(deployer_contract, 1)], constructor_calldata.clone(), true);
    let class_hash = deployer_contract.get_class_hash();

    // No errors expected.
    let contract_address = calculate_contract_address(
        ContractAddressSalt::default(),
        class_hash,
        &Calldata(constructor_calldata.clone().into()),
        deployer_contract.get_instance_address(0),
    )
    .unwrap();

    let deploy_call = &entry_point_call.execute_directly(&mut state).unwrap();

    assert_eq!(
        deploy_call.execution,
        CallExecution { retdata: retdata![], gas_consumed: 188780, ..CallExecution::default() }
    );

    let constructor_call = &deploy_call.inner_calls[0];

    assert_eq!(constructor_call.call.storage_address, contract_address);
    assert_eq!(
        constructor_call.execution,
        CallExecution {
            // The test contract constructor returns its first argument.
            retdata: retdata![constructor_calldata[0]],
            // This reflects the gas cost of storage write syscall.
            gas_consumed: 15140,
            ..CallExecution::default()
        }
    );
    assert_eq!(state.get_class_hash_at(contract_address).unwrap(), class_hash);
}

#[test_case(RunnableCairo1::Casm;"VM")]
#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native;"Native"))]
fn to_unavailable_address(runnable_version: RunnableCairo1) {
    let deployer_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let constructor_calldata = vec![
        felt!(1_u8), // Calldata: address.
        felt!(1_u8), // Calldata: value.
    ];
    let (entry_point_call, _deployed_addr, mut state) =
        setup_deploy_call(&[(deployer_contract, 1)], constructor_calldata.clone(), true);

    entry_point_call.clone().execute_directly(&mut state).unwrap();
    let error = entry_point_call.execute_directly(&mut state).unwrap_err().to_string();

    assert!(error.contains("Deployment failed:"));
}

/// Test that call data length affects the call info resources.
/// Specifcly every argument in the call data add 1 pedersen builtin.
#[test_case(CairoVersion::Cairo1(RunnableCairo1::Casm);"Cairo1-VM")]
#[test_case(CairoVersion::Cairo0;"Cairo0")]
fn calldata_length(cairo_version: CairoVersion) {
    // Test contract: (constructor gets 2 arguments)
    let test_contract = FeatureContract::TestContract(cairo_version);
    // Account contract: (constructor gets 1 argument)
    let account_contract = FeatureContract::FaultyAccount(cairo_version);
    // Empty contract.
    let empty_contract = FeatureContract::Empty(cairo_version);

    let mut state = test_state(
        &ChainInfo::create_for_testing(),
        Fee(0),
        &[(test_contract, 1), (account_contract, 0), (empty_contract, 0)],
    );

    // Use the maximum sierra version to avoid using sierra gas as the tracked resource.
    let max_sierra_version = SierraVersion::new(u64::MAX, u64::MAX, u64::MAX);
    let mut block_context = BlockContext::create_for_testing();
    block_context.versioned_constants.min_sierra_version_for_sierra_gas = max_sierra_version;

    // Flag of deploy syscall.
    let deploy_from_zero = true;

    // Deploy account contract.
    let account_constructor_calldata = vec![felt!(0_u8)];
    let calldata = calldata_for_deploy_test(
        account_contract.get_class_hash(),
        &account_constructor_calldata,
        deploy_from_zero,
    );
    let deploy_account_call = CallEntryPoint {
        entry_point_selector: selector_from_name("test_deploy"),
        calldata,
        ..trivial_external_entry_point_new(test_contract)
    };
    let deploy_account_call_info = &deploy_account_call
        .execute_directly_given_block_context(&mut state, block_context.clone())
        .unwrap();

    // Deploy test contract.
    let test_constructor_calldata = vec![felt!(1_u8), felt!(1_u8)];
    let calldata = calldata_for_deploy_test(
        test_contract.get_class_hash(),
        &test_constructor_calldata,
        deploy_from_zero,
    );
    let deploy_test_call = CallEntryPoint {
        entry_point_selector: selector_from_name("test_deploy"),
        calldata,
        ..trivial_external_entry_point_new(test_contract)
    };
    let deploy_test_call_info = deploy_test_call
        .execute_directly_given_block_context(&mut state, block_context.clone())
        .unwrap();

    // Deploy empty contract.
    let calldata = calldata_for_deploy_test(empty_contract.get_class_hash(), &[], deploy_from_zero);
    let deploy_empty_call = CallEntryPoint {
        entry_point_selector: selector_from_name("test_deploy"),
        calldata,
        ..trivial_external_entry_point_new(test_contract)
    };
    let deploy_empty_call_info = deploy_empty_call
        .execute_directly_given_block_context(&mut state, block_context.clone())
        .unwrap();

    // Extract pedersen counter from each call.
    let deploy_empty_call_pedersen = deploy_empty_call_info
        .resources
        .builtin_instance_counter
        .get(&BuiltinName::pedersen)
        .copied()
        .unwrap();

    let deploy_account_pedersen = deploy_account_call_info
        .resources
        .builtin_instance_counter
        .get(&BuiltinName::pedersen)
        .copied()
        .unwrap();
    let deploy_test_pedersen = deploy_test_call_info
        .resources
        .builtin_instance_counter
        .get(&BuiltinName::pedersen)
        .copied()
        .unwrap();

    // Verify that pedersen cost = base_pedersen cost +
    // deploy_syscall_linear_factor_cost*linear_factor.
    let deploy_syscall_base_pedersen_cost = block_context
        .versioned_constants
        .get_additional_os_syscall_resources(&HashMap::from([(
            SyscallSelector::Deploy,
            (SyscallUsage::new(1, 0)),
        )]))
        .builtin_instance_counter
        .get(&BuiltinName::pedersen)
        .copied()
        .unwrap();
    let deploy_syscall_linear_factor_cost = block_context
        .versioned_constants
        .get_additional_os_syscall_resources(&HashMap::from([(
            SyscallSelector::Deploy,
            (SyscallUsage::new(0, 1)),
        )]))
        .builtin_instance_counter
        .get(&BuiltinName::pedersen)
        .copied()
        .unwrap();

    assert!(
        deploy_syscall_base_pedersen_cost
            + test_constructor_calldata.len() * deploy_syscall_linear_factor_cost
            == deploy_test_pedersen
    );
    assert!(
        deploy_syscall_base_pedersen_cost
            + account_constructor_calldata.len() * deploy_syscall_linear_factor_cost
            == deploy_account_pedersen
    );
    assert!(deploy_empty_call_pedersen == deploy_syscall_base_pedersen_cost);
}

#[test_case(RunnableCairo1::Casm;"VM")]
#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native;"Native"))]
fn reject_deploy_in_validate_mode(runnable_version: RunnableCairo1) {
    let deployer_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let empty_contract = FeatureContract::Empty(CairoVersion::Cairo1(runnable_version));

    let (entry_point_call, _deployed_addr, mut state) =
        setup_deploy_call(&[(deployer_contract, 1), (empty_contract, 0)], vec![], true);

    // Reject the deploy syscall in validate mode.
    let error = entry_point_call.clone().execute_directly_in_validate_mode(&mut state).unwrap_err();
    assert!(error.to_string().contains("Unauthorized syscall deploy in execution mode Validate."));
}
