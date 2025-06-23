use std::collections::HashMap;
use std::sync::Arc;

use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use cairo_vm::types::builtin_name::BuiltinName;
use expect_test::expect;
use pretty_assertions::assert_eq;
use rstest::rstest;
use starknet_api::contract_class::SierraVersion;
use starknet_api::core::calculate_contract_address;
use starknet_api::transaction::fields::{Calldata, ContractAddressSalt, Fee};
use starknet_api::{calldata, felt};
use test_case::test_case;

use crate::context::{BlockContext, ChainInfo};
use crate::execution::call_info::CallExecution;
use crate::execution::common_hints::ExecutionMode;
use crate::execution::syscalls::vm_syscall_utils::{SyscallSelector, SyscallUsage};
use crate::retdata;
use crate::state::state_api::StateReader;
use crate::test_utils::create_deploy_entry_point;
use crate::test_utils::initial_test_state::test_state;
use crate::transaction::objects::{CurrentTransactionInfo, TransactionInfo};

#[test_case(RunnableCairo1::Casm;"VM")]
#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native;"Native"))]
fn no_constructor(runnable_version: RunnableCairo1) {
    // TODO(Yoni): share the init code of the tests in this file.
    let deployer_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let empty_contract = FeatureContract::Empty(CairoVersion::Cairo1(runnable_version));
    let class_hash = empty_contract.get_class_hash();

    let mut state = test_state(
        &ChainInfo::create_for_testing(),
        Fee(0),
        &[(deployer_contract, 1), (empty_contract, 0)],
    );

    let entry_point_call = create_deploy_entry_point(class_hash, &[], true, deployer_contract);

    let deploy_call = &entry_point_call.execute_directly(&mut state).unwrap();
    expect![[r#"
        CallExecution {
            retdata: Retdata(
                [],
            ),
            events: [],
            l2_to_l1_messages: [],
            failed: false,
            gas_consumed: 156540,
            cairo_native: runnable_version.is_cairo_native(),
        }
    "#]]
    .assert_debug_eq(&deploy_call.execution);
    assert_eq!(deploy_call.execution.retdata, retdata![]);

    let deployed_contract_address = calculate_contract_address(
        ContractAddressSalt::default(),
        class_hash,
        &calldata![],
        deployer_contract.get_instance_address(0),
    )
    .unwrap();

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
    let class_hash = empty_contract.get_class_hash();

    let mut state = test_state(
        &ChainInfo::create_for_testing(),
        Fee(0),
        &[(deployer_contract, 1), (empty_contract, 0)],
    );

    let entry_point_call =
        create_deploy_entry_point(class_hash, &[felt!(1_u8), felt!(1_u8)], true, deployer_contract);

    let error = entry_point_call.execute_directly(&mut state).unwrap_err().to_string();
    assert!(error.contains(
        "Invalid input: constructor_calldata; Cannot pass calldata to a contract with no \
         constructor."
    ));
}

#[test_case(RunnableCairo1::Casm;"VM")]
#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native;"Native"))]
fn with_constructor(runnable_version: RunnableCairo1) {
    let deployer_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let mut state = test_state(&ChainInfo::create_for_testing(), Fee(0), &[(deployer_contract, 1)]);

    let class_hash = deployer_contract.get_class_hash();
    let constructor_calldata = vec![
        felt!(1_u8), // Calldata: address.
        felt!(1_u8), // Calldata: value.
    ];

    let entry_point_call =
        create_deploy_entry_point(class_hash, &constructor_calldata, true, deployer_contract);

    // No errors expected.
    let contract_address = calculate_contract_address(
        ContractAddressSalt::default(),
        class_hash,
        &Calldata(constructor_calldata.clone().into()),
        deployer_contract.get_instance_address(0),
    )
    .unwrap();

    let deploy_call = &entry_point_call.execute_directly(&mut state).unwrap();

    expect![[r#"
        CallExecution {
            retdata: Retdata(
                [],
            ),
            events: [],
            l2_to_l1_messages: [],
            failed: false,
            gas_consumed: 184620,
            cairo_native: runnable_version.is_cairo_native(),
        }
    "#]]
    .assert_debug_eq(&deploy_call.execution);
    assert_eq!(deploy_call.execution.retdata, retdata![]);

    let constructor_call = &deploy_call.inner_calls[0];

    expect![[r#"
        CallExecution {
            retdata: Retdata(
                [
                    0x1,
                ],
            ),
            events: [],
            l2_to_l1_messages: [],
            failed: false,
            gas_consumed: 14640,
            cairo_native: runnable_version.is_cairo_native(),
        }
    "#]]
    .assert_debug_eq(&constructor_call.execution);
    assert_eq!(constructor_call.execution.retdata, retdata![constructor_calldata[0]]);
    assert_eq!(constructor_call.call.storage_address, contract_address);

    assert_eq!(state.get_class_hash_at(contract_address).unwrap(), class_hash);
}

#[test_case(RunnableCairo1::Casm;"VM")]
#[cfg_attr(feature = "cairo_native", test_case(RunnableCairo1::Native;"Native"))]
fn to_unavailable_address(runnable_version: RunnableCairo1) {
    let deployer_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let mut state = test_state(&ChainInfo::create_for_testing(), Fee(0), &[(deployer_contract, 1)]);

    let class_hash = deployer_contract.get_class_hash();
    let constructor_calldata = vec![
        felt!(1_u8), // Calldata: address.
        felt!(1_u8), // Calldata: value.
    ];

    let entry_point_call =
        create_deploy_entry_point(class_hash, &constructor_calldata, true, deployer_contract);

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

    let deploy_account_call = create_deploy_entry_point(
        account_contract.get_class_hash(),
        &account_constructor_calldata,
        deploy_from_zero,
        test_contract,
    );

    let deploy_account_call_info = &deploy_account_call
        .execute_directly_given_block_context(&mut state, block_context.clone())
        .unwrap();

    // Deploy test contract.
    let test_constructor_calldata = vec![felt!(1_u8), felt!(1_u8)];

    let deploy_test_call = create_deploy_entry_point(
        test_contract.get_class_hash(),
        &test_constructor_calldata,
        deploy_from_zero,
        test_contract,
    );

    let deploy_test_call_info = deploy_test_call
        .execute_directly_given_block_context(&mut state, block_context.clone())
        .unwrap();

    // Deploy empty contract.
    let deploy_empty_call = create_deploy_entry_point(
        empty_contract.get_class_hash(),
        &[],
        deploy_from_zero,
        test_contract,
    );

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

#[rstest]
#[case::cairo0(CairoVersion::Cairo0)]
#[case::cairo1_vm(CairoVersion::Cairo1(RunnableCairo1::Casm))]
#[cfg_attr(
    feature = "cairo_native",
    case::cairo1_native(CairoVersion::Cairo1(RunnableCairo1::Native))
)]
fn disable_deploy_in_validate_mode_flag_behavior(
    #[case] cairo_version: CairoVersion,
    #[values(true, false)] allow_deploy: bool,
) {
    // TODO(Yoni): share the init code of the tests in this file.
    let deployer_contract = FeatureContract::TestContract(cairo_version);
    let empty_contract = FeatureContract::Empty(cairo_version);
    let class_hash = empty_contract.get_class_hash();
    let mut block_context = BlockContext::create_for_testing();

    // Override default.
    block_context.versioned_constants.disable_deploy_in_validation_mode = !allow_deploy;

    let mut state = test_state(
        &ChainInfo::create_for_testing(),
        Fee(0),
        &[(deployer_contract, 1), (empty_contract, 0)],
    );

    let entry_point_call = create_deploy_entry_point(class_hash, &[], true, deployer_contract);

    let limit_steps_by_resources = false;
    let res = entry_point_call.clone().execute_directly_given_tx_info(
        &mut state,
        TransactionInfo::Current(CurrentTransactionInfo::create_for_testing()),
        Some(Arc::new(block_context)),
        limit_steps_by_resources,
        ExecutionMode::Validate,
    );
    if allow_deploy {
        assert!(res.is_ok());
    } else {
        assert!(res.is_err());
        assert!(
            res.unwrap_err()
                .to_string()
                .contains("Unauthorized syscall deploy in execution mode Validate."),
        );
    }
}
