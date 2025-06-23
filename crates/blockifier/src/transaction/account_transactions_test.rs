use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use assert_matches::assert_matches;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::{create_calldata, create_trivial_calldata};
use blockifier_test_utils::contracts::FeatureContract;
use cairo_lang_runner::short_string::as_cairo_short_string;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::vm::runners::cairo_runner::ResourceTracker;
use num_traits::Inv;
use pretty_assertions::{assert_eq, assert_ne};
use rstest::rstest;
use rstest_reuse::apply;
use starknet_api::abi::abi_utils::{
    get_fee_token_var_address,
    get_storage_var_address,
    selector_from_name,
};
use starknet_api::block::{FeeType, GasPrice};
use starknet_api::core::{
    calculate_contract_address,
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    Nonce,
    PatriciaKey,
};
use starknet_api::executable_transaction::{
    AccountTransaction as ApiExecutableTransaction,
    DeclareTransaction as ApiExecutableDeclareTransaction,
    TransactionType,
};
use starknet_api::execution_resources::{GasAmount, GasVector};
use starknet_api::hash::StarkHash;
use starknet_api::state::StorageKey;
use starknet_api::test_utils::declare::executable_declare_tx;
use starknet_api::test_utils::deploy_account::executable_deploy_account_tx;
use starknet_api::test_utils::invoke::{executable_invoke_tx, InvokeTxArgs};
use starknet_api::test_utils::{
    NonceManager,
    DEFAULT_L1_DATA_GAS_MAX_AMOUNT,
    DEFAULT_L1_GAS_AMOUNT,
    DEFAULT_L2_GAS_MAX_AMOUNT,
    DEFAULT_STRK_L1_GAS_PRICE,
    DEFAULT_STRK_L2_GAS_PRICE,
    MAX_FEE,
};
use starknet_api::transaction::constants::TRANSFER_ENTRY_POINT_NAME;
use starknet_api::transaction::fields::{
    AllResourceBounds,
    Calldata,
    ContractAddressSalt,
    Fee,
    GasVectorComputationMode,
    Resource,
    ResourceBounds,
    ValidResourceBounds,
};
use starknet_api::transaction::{
    DeclareTransaction,
    DeclareTransactionV2,
    DeclareTransactionV3,
    TransactionHash,
    TransactionVersion,
};
use starknet_api::{
    calldata,
    class_hash,
    contract_address,
    declare_tx_args,
    deploy_account_tx_args,
    felt,
    invoke_tx_args,
    nonce,
    storage_key,
};
use starknet_types_core::felt::Felt;

use crate::check_tx_execution_error_for_invalid_scenario;
use crate::context::{BlockContext, TransactionContext};
use crate::execution::call_info::CallInfo;
use crate::execution::contract_class::TrackedResource;
use crate::execution::entry_point::{EntryPointExecutionContext, SierraGasRevertTracker};
use crate::execution::syscalls::vm_syscall_utils::SyscallSelector;
use crate::fee::fee_utils::{
    get_fee_by_gas_vector,
    get_sequencer_balance_keys,
    GasVectorToL1GasForFee,
};
use crate::fee::gas_usage::estimate_minimal_gas_vector;
use crate::state::cached_state::{
    CachedState,
    StateChangesCount,
    StateChangesCountForFee,
    StateMaps,
    TransactionalState,
};
use crate::state::state_api::{State, StateReader};
use crate::test_utils::contracts::FeatureContractTrait;
use crate::test_utils::dict_state_reader::DictStateReader;
use crate::test_utils::initial_test_state::{fund_account, test_state};
use crate::test_utils::syscall::build_recurse_calldata;
use crate::test_utils::test_templates::cairo_version;
use crate::test_utils::{
    get_const_syscall_resources,
    get_tx_resources,
    CompilerBasedVersion,
    BALANCE,
};
use crate::transaction::account_transaction::{
    AccountTransaction,
    ExecutionFlags as AccountExecutionFlags,
};
use crate::transaction::errors::{TransactionExecutionError, TransactionPreValidationError};
use crate::transaction::objects::{
    HasRelatedFeeType,
    TransactionExecutionInfo,
    TransactionInfoCreator,
};
use crate::transaction::test_utils::{
    all_resource_bounds,
    block_context,
    calculate_class_info_for_testing,
    create_account_tx_for_validate_test_nonce_0,
    create_all_resource_bounds,
    create_gas_amount_bounds_with_default_price,
    create_test_init_data,
    default_all_resource_bounds,
    default_l1_resource_bounds,
    deploy_and_fund_account,
    invoke_tx_with_default_flags,
    l1_resource_bounds,
    max_fee,
    run_invoke_tx,
    FaultyAccountTxCreatorArgs,
    TestInitData,
    INVALID,
};
use crate::transaction::transactions::ExecutableTransaction;
use crate::utils::u64_from_usize;

#[rstest]
#[case::cairo1(CairoVersion::Cairo1(RunnableCairo1::Casm))]
#[cfg_attr(
    feature = "cairo_native",
    case::cairo1_native(CairoVersion::Cairo1(RunnableCairo1::Native))
)]
fn test_circuit(
    block_context: BlockContext,
    default_all_resource_bounds: ValidResourceBounds,
    #[case] cairo_version: CairoVersion,
) {
    let TestInitData { mut state, account_address, contract_address, mut nonce_manager } =
        create_test_init_data(&block_context.chain_info, cairo_version);

    // Invoke a function that changes the state and reverts.
    let tx_args = invoke_tx_args! {
        sender_address: account_address,
        calldata: create_calldata(
            contract_address,
                "test_circuit",
                &[]
            ),
        nonce: nonce_manager.next(account_address)
    };
    let tx_execution_info = run_invoke_tx(
        &mut state,
        &block_context,
        invoke_tx_args! {
            resource_bounds: default_all_resource_bounds,
            ..tx_args
        },
    )
    .unwrap();

    assert!(tx_execution_info.revert_error.is_none());
}

#[rstest]
#[case::vm(default_l1_resource_bounds(), CairoVersion::Cairo1(RunnableCairo1::Casm))]
#[case::gas(default_all_resource_bounds(), CairoVersion::Cairo1(RunnableCairo1::Casm))]
#[cfg_attr(
    feature = "cairo_native",
    case::gas_native(default_all_resource_bounds(), CairoVersion::Cairo1(RunnableCairo1::Native))
)]
fn test_rc96_holes(
    block_context: BlockContext,
    #[case] resource_bounds: ValidResourceBounds,
    #[case] cairo_version: CairoVersion,
) {
    let TestInitData { mut state, account_address, contract_address, mut nonce_manager } =
        create_test_init_data(&block_context.chain_info, cairo_version);

    // Invoke a function that changes the state and reverts.
    let tx_args = invoke_tx_args! {
        sender_address: account_address,
        calldata: create_calldata(
            contract_address,
                "test_rc96_holes",
                &[]
            ),
        nonce: nonce_manager.next(account_address)
    };
    let tx_execution_info = run_invoke_tx(
        &mut state,
        &block_context,
        invoke_tx_args! {
            resource_bounds: resource_bounds,
            ..tx_args
        },
    )
    .unwrap();

    assert!(!tx_execution_info.is_reverted());
    if tx_execution_info.validate_call_info.unwrap().tracked_resource == TrackedResource::CairoSteps
    {
        assert_eq!(
            tx_execution_info.receipt.resources.computation.vm_resources.builtin_instance_counter
                [&BuiltinName::range_check96],
            24
        );
    }
}

#[rstest]
#[case::deprecated_tx(TransactionVersion::ONE, GasVectorComputationMode::NoL2Gas)]
#[case::l1_bounds(TransactionVersion::THREE, GasVectorComputationMode::NoL2Gas)]
#[case::all_bounds(TransactionVersion::THREE, GasVectorComputationMode::All)]
fn test_fee_enforcement(
    block_context: BlockContext,
    #[case] version: TransactionVersion,
    #[case] gas_bounds_mode: GasVectorComputationMode,
    #[values(true, false)] zero_bounds: bool,
) {
    let account = FeatureContract::AccountWithoutValidations(CairoVersion::Cairo0);
    let state = &mut test_state(&block_context.chain_info, BALANCE, &[(account, 1)]);
    let tx = executable_deploy_account_tx(deploy_account_tx_args! {
        class_hash: account.get_class_hash(),
        max_fee: Fee(if zero_bounds { 0 } else { MAX_FEE.0 }),
        resource_bounds: match gas_bounds_mode {
            GasVectorComputationMode::NoL2Gas => l1_resource_bounds(
                (if zero_bounds { 0 } else { DEFAULT_L1_GAS_AMOUNT.0 }).into(),
                DEFAULT_STRK_L1_GAS_PRICE.into()
            ),
            GasVectorComputationMode::All => create_gas_amount_bounds_with_default_price(
                GasVector{
                    l1_gas: (if zero_bounds { 0 } else { DEFAULT_L1_GAS_AMOUNT.0 }).into(),
                    l2_gas: (if zero_bounds { 0 } else { DEFAULT_L2_GAS_MAX_AMOUNT.0 }).into(),
                    l1_data_gas: (if zero_bounds { 0 } else { DEFAULT_L1_DATA_GAS_MAX_AMOUNT.0 }).into(),
                },
            ),
        },
        version,
    });
    let deploy_account_tx = AccountTransaction::new_for_sequencing(tx);

    let enforce_fee = deploy_account_tx.enforce_fee();
    assert_ne!(zero_bounds, enforce_fee);
    let result = deploy_account_tx.execute(state, &block_context);
    // When fee is enforced, execution should fail because the account doesn't have sufficient
    // balance.
    if enforce_fee {
        assert_matches!(
            result,
            Err(TransactionExecutionError::TransactionPreValidationError(
                TransactionPreValidationError::TransactionFeeError(_)
            ))
        );
    } else {
        assert!(result.is_ok());
    }
}

#[rstest]
fn test_all_bounds_combinations_enforce_fee(
    #[values(0, 1)] l1_gas_bound: u64,
    #[values(0, 1)] l1_data_gas_bound: u64,
    #[values(0, 1)] l2_gas_bound: u64,
) {
    let expected_enforce_fee = l1_gas_bound + l1_data_gas_bound + l2_gas_bound > 0;
    let account_tx = invoke_tx_with_default_flags(invoke_tx_args! {
        version: TransactionVersion::THREE,
        resource_bounds: create_gas_amount_bounds_with_default_price(
            GasVector {
                l1_gas: l1_gas_bound.into(),
                l2_gas: l2_gas_bound.into(),
                l1_data_gas: l1_data_gas_bound.into(),
            },
        ),
    });
    assert_eq!(account_tx.enforce_fee(), expected_enforce_fee);
}

#[rstest]
#[case::positive_case_deprecated_tx(true, true)]
#[case::positive_case_new_tx(true, false)]
#[should_panic(expected = "exceeded bounds; max fee is")]
#[case::negative_case_deprecated_tx(false, true)]
#[should_panic(expected = "exceeded bounds; max possible fee is")]
#[case::negative_case_new_tx(false, false)]
fn test_assert_actual_fee_in_bounds(
    block_context: BlockContext,
    #[case] positive_flow: bool,
    #[case] deprecated_tx: bool,
) {
    let actual_fee_offset = Fee(if positive_flow { 0 } else { 1 });
    if deprecated_tx {
        let max_fee = Fee(100);
        let tx = invoke_tx_with_default_flags(
            invoke_tx_args! { max_fee, version: TransactionVersion::ONE },
        );
        let context = Arc::new(block_context.to_tx_context(&tx));
        AccountTransaction::assert_actual_fee_in_bounds(&context, max_fee + actual_fee_offset);
    } else {
        // All resources.
        let l1_gas = ResourceBounds { max_amount: GasAmount(2), max_price_per_unit: GasPrice(3) };
        let l2_gas = ResourceBounds { max_amount: GasAmount(4), max_price_per_unit: GasPrice(5) };
        let l1_data_gas =
            ResourceBounds { max_amount: GasAmount(6), max_price_per_unit: GasPrice(7) };
        let all_resource_bounds =
            ValidResourceBounds::AllResources(AllResourceBounds { l1_gas, l2_gas, l1_data_gas });
        let all_resource_fee = l1_gas.max_amount.checked_mul(l1_gas.max_price_per_unit).unwrap()
            + l2_gas.max_amount.checked_mul(l2_gas.max_price_per_unit).unwrap()
            + l1_data_gas.max_amount.checked_mul(l1_data_gas.max_price_per_unit).unwrap()
            + actual_fee_offset;

        // L1 resources.
        let l1_resource_bounds = ValidResourceBounds::L1Gas(l1_gas);
        let l1_resource_fee =
            l1_gas.max_amount.checked_mul(l1_gas.max_price_per_unit).unwrap() + actual_fee_offset;

        for (bounds, actual_fee) in
            [(all_resource_bounds, all_resource_fee), (l1_resource_bounds, l1_resource_fee)]
        {
            let tx = invoke_tx_with_default_flags(invoke_tx_args! {
                resource_bounds: bounds,
                version: TransactionVersion::THREE,
            });
            let context = Arc::new(block_context.to_tx_context(&tx));
            AccountTransaction::assert_actual_fee_in_bounds(&context, actual_fee);
        }
    }
}

// TODO(Dori, 15/9/2023): Convert version variance to attribute macro.
#[rstest]
#[case::v0(TransactionVersion::ZERO, default_all_resource_bounds())]
#[case::v1(TransactionVersion::ONE, default_all_resource_bounds())]
#[case::l1_bounds(TransactionVersion::THREE, default_l1_resource_bounds())]
#[case::all_bounds(TransactionVersion::THREE, default_all_resource_bounds())]
fn test_account_flow_test(
    block_context: BlockContext,
    max_fee: Fee,
    #[case] tx_version: TransactionVersion,
    #[case] resource_bounds: ValidResourceBounds,
) {
    let TestInitData { mut state, account_address, contract_address, mut nonce_manager } =
        create_test_init_data(&block_context.chain_info, CairoVersion::Cairo0);

    // Invoke a function from the newly deployed contract.
    run_invoke_tx(
        &mut state,
        &block_context,
        invoke_tx_args! {
            max_fee,
            sender_address: account_address,
            calldata: create_trivial_calldata(contract_address),
            version: tx_version,
            resource_bounds,
            nonce: nonce_manager.next(account_address),
        },
    )
    .unwrap();
}

#[rstest]
#[case(TransactionVersion::ZERO)]
#[case(TransactionVersion::ONE)]
#[case(TransactionVersion::THREE)]
fn test_invoke_tx_from_non_deployed_account(
    block_context: BlockContext,
    max_fee: Fee,
    default_all_resource_bounds: ValidResourceBounds,
    #[case] tx_version: TransactionVersion,
) {
    let TestInitData { mut state, account_address, contract_address: _, mut nonce_manager } =
        create_test_init_data(&block_context.chain_info, CairoVersion::Cairo0);
    // Invoke a function from the newly deployed contract.
    let entry_point_selector = selector_from_name("return_result");

    let non_deployed_contract_address = StarkHash::TWO;

    let tx_result = run_invoke_tx(
        &mut state,
        &block_context,
        invoke_tx_args! {
            max_fee,
            sender_address: account_address,
            calldata: calldata![
                non_deployed_contract_address, // Contract address.
                entry_point_selector.0,    // EP selector.
                felt!(1_u8),         // Calldata length.
                felt!(2_u8)          // Calldata: num.
            ],
            resource_bounds: default_all_resource_bounds,
            version: tx_version,
            nonce: nonce_manager.next(account_address),
        },
    );
    let expected_error = "is not deployed.";
    match tx_result {
        Ok(info) => {
            //  Make sure the error is because the account wasn't deployed.
            assert!(info.revert_error.unwrap().to_string().contains(expected_error));
        }
        Err(err) => {
            //  Make sure the error is because the account wasn't deployed.
            assert!(err.to_string().contains(expected_error));
            // We expect to get an error only when tx_version is 0, on other versions to revert.
            assert_eq!(tx_version, TransactionVersion::ZERO);
        }
    }
}

#[rstest]
// Try two runs for each recursion type: one short run (success), and one that reverts due to step
// limit.
fn test_infinite_recursion(
    #[values(true, false)] success: bool,
    #[values(true, false)] normal_recurse: bool,
    mut block_context: BlockContext,
    #[values(default_l1_resource_bounds(), default_all_resource_bounds())]
    resource_bounds: ValidResourceBounds,
) {
    // Limit the number of execution steps (so we quickly hit the limit).
    block_context.versioned_constants.invoke_tx_max_n_steps = 4700;

    let TestInitData { mut state, account_address, contract_address, mut nonce_manager } =
        create_test_init_data(&block_context.chain_info, CairoVersion::Cairo0);

    let recursion_depth = if success { 3_u32 } else { 1000_u32 };

    let execute_calldata = if normal_recurse {
        create_calldata(contract_address, "recurse", &[felt!(recursion_depth)])
    } else {
        create_calldata(
            contract_address,
            "recursive_syscall",
            &[
                *contract_address.0.key(), // Calldata: raw contract address.
                selector_from_name("recursive_syscall").0, // Calldata: raw selector
                felt!(recursion_depth),
            ],
        )
    };

    let tx_execution_info = run_invoke_tx(
        &mut state,
        &block_context,
        invoke_tx_args! {
            resource_bounds,
            sender_address: account_address,
            calldata: execute_calldata,
            nonce: nonce_manager.next(account_address),
        },
    )
    .unwrap();
    if success {
        assert!(tx_execution_info.revert_error.is_none());
    } else {
        assert!(
            tx_execution_info
                .revert_error
                .unwrap()
                .to_string()
                .contains("RunResources has no remaining steps.")
        );
    }
}

/// Tests that validation fails on out-of-gas if L2 gas bound is too low.
#[rstest]
fn test_max_fee_limit_validate(
    mut block_context: BlockContext,
    default_all_resource_bounds: ValidResourceBounds,
    #[values(true, false)] use_kzg_da: bool,
) {
    let resource_bounds = default_all_resource_bounds;
    block_context.block_info.use_kzg_da = use_kzg_da;
    let chain_info = block_context.chain_info.clone();
    let gas_computation_mode = resource_bounds.get_gas_vector_computation_mode();
    let TestInitData { mut state, account_address, contract_address, mut nonce_manager } =
        create_test_init_data(&chain_info, CairoVersion::Cairo1(RunnableCairo1::Casm));
    let grindy_validate_account =
        FeatureContract::AccountWithLongValidate(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let grindy_class_hash = grindy_validate_account.get_class_hash();
    let block_info = block_context.block_info.clone();
    let class_info = calculate_class_info_for_testing(grindy_validate_account.get_class());

    // Declare the grindy-validation account.
    let tx = executable_declare_tx(
        declare_tx_args! {
            class_hash: grindy_class_hash,
            sender_address: account_address,
            resource_bounds,
            nonce: nonce_manager.next(account_address),
        },
        class_info,
    );
    let account_tx = AccountTransaction::new_with_default_flags(tx);
    account_tx.execute(&mut state, &block_context).unwrap();

    // Deploy grindy account with a lot of grind in the constructor.
    // Expect this to fail without bumping nonce, so pass a temporary nonce manager.
    // We want to test the block step / gas bounds here - so set them to something low.
    let old_validate_max_n_steps = block_context.versioned_constants.validate_max_n_steps;
    block_context.versioned_constants.validate_max_n_steps = 1000;
    let old_validate_max_gas =
        block_context.versioned_constants.os_constants.validate_max_sierra_gas;
    block_context.set_sierra_gas_limits(None, Some(GasAmount(100000)));

    let mut ctor_grind_arg = felt!(1_u8); // Grind in deploy phase.
    let ctor_storage_arg = felt!(1_u8); // Not relevant for this test.
    let (deploy_account_tx, _) = deploy_and_fund_account(
        &mut state,
        &mut NonceManager::default(),
        &chain_info,
        deploy_account_tx_args! {
            class_hash: grindy_class_hash,
            resource_bounds,
            constructor_calldata: calldata![ctor_grind_arg, ctor_storage_arg],
        },
    );
    let error_trace =
        deploy_account_tx.execute(&mut state, &block_context).unwrap_err().to_string();
    assert!(error_trace.contains("Out of gas"));
    block_context.versioned_constants.validate_max_n_steps = old_validate_max_n_steps;
    block_context.set_sierra_gas_limits(None, Some(old_validate_max_gas));

    // Deploy grindy account successfully this time.
    ctor_grind_arg = felt!(0_u8); // Do not grind in deploy phase.
    let (deploy_account_tx, grindy_account_address) = deploy_and_fund_account(
        &mut state,
        &mut nonce_manager,
        &chain_info,
        deploy_account_tx_args! {
            class_hash: grindy_class_hash,
            resource_bounds,
            constructor_calldata: calldata![ctor_grind_arg, ctor_storage_arg],
        },
    );
    deploy_account_tx.execute(&mut state, &block_context).unwrap();

    // Invoke a function that grinds validate (any function will do); set bounds low enough to fail
    // on this grind.
    // Only grind a small number of iterations (in the calldata) to ensure we are limited by the
    // transaction bounds, and not the global block bounds.
    // To ensure bounds are low enough, estimate minimal resources consumption, and set bounds
    // slightly above them.
    let tx_args = invoke_tx_args! {
        sender_address: grindy_account_address,
        calldata: create_calldata(contract_address, "return_result", &[1000_u32.into()]),
        version: TransactionVersion::THREE,
        nonce: nonce_manager.next(grindy_account_address)
    };

    let account_tx = invoke_tx_with_default_flags(invoke_tx_args! {
        // Temporary upper bounds; just for gas estimation.
        max_fee: MAX_FEE,
        resource_bounds,
        ..tx_args.clone()
    });
    let estimated_min_gas_usage_vector =
        estimate_minimal_gas_vector(&block_context, &account_tx, &gas_computation_mode);
    let tip = block_context.to_tx_context(&account_tx).effective_tip();
    let estimated_min_fee = get_fee_by_gas_vector(
        &block_info,
        estimated_min_gas_usage_vector,
        &account_tx.fee_type(),
        tip,
    );

    // Make sure the resource bounds are the limiting factor by blowing up the block bounds.
    let old_validate_max_n_steps = block_context.versioned_constants.validate_max_n_steps;
    block_context.versioned_constants.validate_max_n_steps = u32::MAX;
    let error_trace = run_invoke_tx(
        &mut state,
        &block_context,
        invoke_tx_args! {
            max_fee: estimated_min_fee,
            resource_bounds: match resource_bounds.get_gas_vector_computation_mode() {
                GasVectorComputationMode::NoL2Gas => {
                    // If KZG DA mode is active, the L1 gas amount in the minimal fee estimate does
                    // not include DA. To cover minimal cost with only an L1 gas bound, need to
                    // convert the L1 data gas to L1 gas.
                    let tx_context = TransactionContext {
                        block_context: Arc::new(block_context.clone()),
                        tx_info: account_tx.create_tx_info(),
                    };
                    let gas_prices = tx_context.get_gas_prices();
                    l1_resource_bounds(
                        estimated_min_gas_usage_vector.to_l1_gas_for_fee(
                            gas_prices, &tx_context.block_context.versioned_constants
                        ),
                        gas_prices.l1_gas_price.into(),
                    )
                }
                GasVectorComputationMode::All => create_all_resource_bounds(
                    estimated_min_gas_usage_vector.l1_gas,
                    block_info.gas_prices
                        .l1_gas_price(&account_tx.fee_type()).into(),
                    estimated_min_gas_usage_vector.l2_gas,
                    block_info.gas_prices
                        .l2_gas_price(&account_tx.fee_type()).into(),
                    estimated_min_gas_usage_vector.l1_data_gas,
                    block_info.gas_prices
                        .l1_data_gas_price(&account_tx.fee_type()).into(),
                ),
            },
            ..tx_args
        },
    )
    .unwrap_err()
    .to_string();
    block_context.versioned_constants.validate_max_n_steps = old_validate_max_n_steps;
    assert!(error_trace.contains("no remaining steps") | error_trace.contains("Out of gas"))
}

#[apply(cairo_version)]
#[case::v1(TransactionVersion::ONE, default_all_resource_bounds())]
#[case::l1_bounds(TransactionVersion::THREE, default_l1_resource_bounds())]
#[case::all_bounds(TransactionVersion::THREE, default_all_resource_bounds())]
fn test_recursion_depth_exceeded(
    #[case] tx_version: TransactionVersion,
    cairo_version: CairoVersion,
    mut block_context: BlockContext,
    max_fee: Fee,
    #[case] resource_bounds: ValidResourceBounds,
) {
    let TestInitData { mut state, account_address, contract_address, mut nonce_manager } =
        create_test_init_data(&block_context.chain_info, cairo_version);

    // Positive test

    // Technical details for this specific recursive entry point:
    // The maximum inner recursion depth is reduced by 2 from the global entry point limit for two
    // reasons:
    // 1. An additional call is made initially before entering the recursion.
    // 2. The base case for recursion occurs at depth 0, not at depth 1.

    // TODO(Ori, 1/2/2024): Write an indicative expect message explaining why the conversion works.
    // This test is highly sensitive to the recursion depth; set the limit to a low value to avoid
    // breaking this test in unusual ways (rust version upgrade, VM feature activation and cairo
    // native feature are all known examples of changes that can cause this test to overflow the
    // stack).
    block_context.versioned_constants.max_recursion_depth = 25;
    let max_inner_recursion_depth: u8 = (block_context.versioned_constants.max_recursion_depth - 2)
        .try_into()
        .expect("Failed to convert usize to u8.");

    let recursive_syscall_entry_point_name = "recursive_syscall";
    let calldata = create_calldata(
        contract_address,
        recursive_syscall_entry_point_name,
        &[
            *contract_address.0.key(), // Calldata: raw contract address.
            selector_from_name(recursive_syscall_entry_point_name).0, // Calldata: raw selector.
            felt!(max_inner_recursion_depth),
        ],
    );
    let invoke_args = invoke_tx_args! {
        max_fee,
        sender_address: account_address,
        calldata,
        version: tx_version,
        nonce: nonce_manager.next(account_address),
        resource_bounds,
    };
    let tx_execution_info = run_invoke_tx(&mut state, &block_context, invoke_args.clone());

    assert!(tx_execution_info.unwrap().revert_error.is_none());

    // Negative test

    let exceeding_recursion_depth = max_inner_recursion_depth + 1;

    let calldata = create_calldata(
        contract_address,
        recursive_syscall_entry_point_name,
        &[
            *contract_address.0.key(), // Calldata: raw contract address.
            selector_from_name(recursive_syscall_entry_point_name).0, // Calldata: raw selector.
            felt!(exceeding_recursion_depth),
        ],
    );
    let invoke_args =
        InvokeTxArgs { calldata, nonce: nonce_manager.next(account_address), ..invoke_args };
    let tx_execution_info = run_invoke_tx(&mut state, &block_context, invoke_args);

    assert!(
        tx_execution_info
            .unwrap()
            .revert_error
            .unwrap()
            .to_string()
            .contains("recursion depth exceeded")
    );
}

#[rstest]
/// Tests that an account invoke transaction that fails the execution phase, still incurs a nonce
/// increase and a fee deduction.
#[case(TransactionVersion::ONE, FeeType::Eth)]
#[case(TransactionVersion::THREE, FeeType::Strk)]
fn test_revert_invoke(
    block_context: BlockContext,
    max_fee: Fee,
    all_resource_bounds: ValidResourceBounds,
    #[case] transaction_version: TransactionVersion,
    #[case] fee_type: FeeType,
) {
    let TestInitData { mut state, account_address, contract_address, mut nonce_manager } =
        create_test_init_data(&block_context.chain_info, CairoVersion::Cairo0);

    // Invoke a function that changes the state and reverts.
    let storage_key = felt!(9_u8);
    let tx_execution_info = run_invoke_tx(
        &mut state,
        &block_context,
        invoke_tx_args! {
            max_fee,
            resource_bounds: all_resource_bounds,
            sender_address: account_address,
            calldata: create_calldata(
                contract_address,
                "write_and_revert",
                // Write some non-zero value.
                &[storage_key, felt!(99_u8)]
            ),
            version: transaction_version,
            nonce: nonce_manager.next(account_address),
        },
    )
    .unwrap();

    // TODO(Dori, 1/7/2023): Verify that the actual fee collected is exactly the fee computed for
    // the validate and fee transfer calls.

    // Check that the transaction was reverted.
    assert!(tx_execution_info.revert_error.is_some());

    // Check that the nonce was increased and the fee was deducted.
    assert_eq!(
        state
            .get_fee_token_balance(
                account_address,
                block_context.chain_info.fee_token_address(&fee_type)
            )
            .unwrap(),
        (felt!(BALANCE.0 - tx_execution_info.receipt.fee.0), felt!(0_u8))
    );
    assert_eq!(state.get_nonce_at(account_address).unwrap(), nonce_manager.next(account_address));

    // Check that reverted steps are taken into account.
    assert!(tx_execution_info.receipt.resources.computation.n_reverted_steps > 0);

    // Check that execution state changes were reverted.
    assert_eq!(
        felt!(0_u8),
        state.get_storage_at(contract_address, StorageKey::try_from(storage_key).unwrap()).unwrap()
    );
}

#[rstest]
#[case::cairo0(CairoVersion::Cairo0)]
#[case::cairo1(CairoVersion::Cairo1(RunnableCairo1::Casm))]
#[cfg_attr(
    feature = "cairo_native",
    case::cairo1_native(CairoVersion::Cairo1(RunnableCairo1::Native))
)]
/// Tests that failing account deployment should not change state (no fee charge or nonce bump).
fn test_fail_deploy_account(
    block_context: BlockContext,
    #[case] cairo_version: CairoVersion,
    #[values(TransactionVersion::ONE, TransactionVersion::THREE)] tx_version: TransactionVersion,
) {
    let chain_info = &block_context.chain_info;
    let faulty_account_feature_contract = FeatureContract::FaultyAccount(cairo_version);
    let state = &mut test_state(chain_info, BALANCE, &[(faulty_account_feature_contract, 0)]);

    // Create and execute (failing) deploy account transaction.
    let deploy_account_tx =
        create_account_tx_for_validate_test_nonce_0(FaultyAccountTxCreatorArgs {
            tx_type: TransactionType::DeployAccount,
            tx_version,
            scenario: INVALID,
            class_hash: faulty_account_feature_contract.get_class_hash(),
            max_fee: BALANCE,
            resource_bounds: default_l1_resource_bounds(),
            ..Default::default()
        });
    let fee_token_address = chain_info.fee_token_address(&deploy_account_tx.fee_type());

    let deploy_address = match &deploy_account_tx.tx {
        ApiExecutableTransaction::DeployAccount(deploy_tx) => deploy_tx.contract_address(),
        _ => unreachable!("deploy_account_tx is a DeployAccount"),
    };
    fund_account(chain_info, deploy_address, Fee(BALANCE.0 * 2), &mut state.state);

    let initial_balance = state.get_fee_token_balance(deploy_address, fee_token_address).unwrap();

    let error = deploy_account_tx.execute(state, &block_context).unwrap_err();
    // Check the error is as expected. Assure the error message is not nonce or fee related.
    check_tx_execution_error_for_invalid_scenario!(cairo_version, error, false);

    // Assert nonce and balance are unchanged, and that no contract was deployed at the address.
    assert_eq!(state.get_nonce_at(deploy_address).unwrap(), nonce!(0_u8));
    assert_eq!(
        state.get_fee_token_balance(deploy_address, fee_token_address).unwrap(),
        initial_balance
    );
    assert_eq!(state.get_class_hash_at(deploy_address).unwrap(), ClassHash::default());
}

#[rstest]
/// Tests that a failing declare transaction should not change state (no fee charge or nonce bump).
fn test_fail_declare(block_context: BlockContext, max_fee: Fee) {
    let chain_info = &block_context.chain_info;
    let TestInitData { mut state, account_address, mut nonce_manager, .. } =
        create_test_init_data(chain_info, CairoVersion::Cairo0);
    let class_hash = class_hash!(0xdeadeadeaf72_u128);
    let contract_class =
        FeatureContract::Empty(CairoVersion::Cairo1(RunnableCairo1::Casm)).get_class();
    let next_nonce = nonce_manager.next(account_address);

    // Cannot fail executing a declare tx unless it's V2 or above, and already declared.
    let declare_tx_v2 = DeclareTransactionV2 {
        max_fee,
        class_hash,
        sender_address: account_address,
        ..Default::default()
    };
    state.set_contract_class(class_hash, contract_class.clone().try_into().unwrap()).unwrap();
    state.set_compiled_class_hash(class_hash, declare_tx_v2.compiled_class_hash).unwrap();
    let class_info = calculate_class_info_for_testing(contract_class);
    let executable_declare = ApiExecutableDeclareTransaction {
        tx: DeclareTransaction::V2(DeclareTransactionV2 { nonce: next_nonce, ..declare_tx_v2 }),
        tx_hash: TransactionHash::default(),
        class_info,
    };
    let declare_account_tx = AccountTransaction::new_with_default_flags(
        ApiExecutableTransaction::Declare(executable_declare),
    );

    // Fail execution, assert nonce and balance are unchanged.
    let tx_info = declare_account_tx.create_tx_info();
    let initial_balance = state
        .get_fee_token_balance(account_address, chain_info.fee_token_address(&tx_info.fee_type()))
        .unwrap();
    declare_account_tx.execute(&mut state, &block_context).unwrap_err();

    assert_eq!(state.get_nonce_at(account_address).unwrap(), next_nonce);
    assert_eq!(
        state
            .get_fee_token_balance(
                account_address,
                chain_info.fee_token_address(&tx_info.fee_type())
            )
            .unwrap(),
        initial_balance
    );
}

#[rstest]
#[case::valid(DeclareTransaction::V3(DeclareTransactionV3 {
    sender_address: ApiExecutableDeclareTransaction::bootstrap_address(),
    class_hash: class_hash!(7_u64),
    compiled_class_hash: CompiledClassHash(8_u64.into()),
    ..Default::default()
}))]
#[should_panic(expected = "UninitializedStorageAddress")]
#[case::wrong_tx_version(DeclareTransaction::V2(DeclareTransactionV2 {
    sender_address: ApiExecutableDeclareTransaction::bootstrap_address(),
    ..Default::default()
}))]
#[should_panic(expected = "InvalidNonce")]
#[case::wrong_nonce(DeclareTransaction::V3(DeclareTransactionV3 {
    sender_address: ApiExecutableDeclareTransaction::bootstrap_address(),
    nonce: Nonce(felt!(1_u64)),
    ..Default::default()
}))]
#[should_panic(expected = "UninitializedStorageAddress")]
#[case::wrong_sender_address(DeclareTransaction::V3(DeclareTransactionV3 {
    sender_address: ContractAddress(PatriciaKey::from(1_u128)),
    ..Default::default()
}))]
#[should_panic(expected = "InsufficientResourceBounds")]
#[case::non_trivial_resource_bounds(DeclareTransaction::V3(DeclareTransactionV3 {
    sender_address: ApiExecutableDeclareTransaction::bootstrap_address(),
    resource_bounds: ValidResourceBounds::AllResources(AllResourceBounds {
        l1_gas: ResourceBounds::default(),
        l2_gas: ResourceBounds{max_amount: GasAmount(1), max_price_per_unit: GasPrice(1)},
        l1_data_gas: ResourceBounds::default(),
    }),
    ..Default::default()
}))]
fn test_bootstrap_declare(block_context: BlockContext, #[case] declare_tx: DeclareTransaction) {
    let class_info = calculate_class_info_for_testing(
        FeatureContract::Empty(CairoVersion::Cairo1(RunnableCairo1::Casm)).get_class(),
    );
    let executable_declare = ApiExecutableDeclareTransaction {
        tx: declare_tx.clone(),
        tx_hash: TransactionHash::default(),
        class_info,
    };
    let declare_account_tx = AccountTransaction::new_for_sequencing(
        ApiExecutableTransaction::Declare(executable_declare),
    );

    let mut state = CachedState::from(DictStateReader::default());
    let res = declare_account_tx.execute(&mut state, &block_context).unwrap();

    // Check declaration.
    assert_eq!(
        state.get_compiled_class_hash(declare_tx.class_hash()).unwrap(),
        declare_tx.compiled_class_hash()
    );

    // Ensure the only change is the class declaration: no fees, nonce bump, etc.
    assert_eq!(res, TransactionExecutionInfo::default());
    assert_eq!(
        state.to_state_diff().unwrap().state_maps,
        StateMaps {
            compiled_class_hashes: HashMap::from([(
                declare_tx.class_hash(),
                declare_tx.compiled_class_hash()
            )]),
            declared_contracts: HashMap::from([(declare_tx.class_hash(), true)]),
            ..Default::default()
        }
    );
}

#[test]
fn test_bootstrap_address() {
    let num = *ApiExecutableDeclareTransaction::bootstrap_address().0.key();
    assert_eq!("BOOTSTRAP", as_cairo_short_string(&num).unwrap());
}

fn recursive_function_calldata(
    contract_address: &ContractAddress,
    depth: u32,
    failure_variant: bool,
) -> Calldata {
    create_calldata(
        *contract_address,
        if failure_variant { "recursive_fail" } else { "recurse" },
        &[felt!(depth)], // Calldata: recursion depth.
    )
}

#[apply(cairo_version)]
/// Tests that reverted transactions are charged more fee and steps than their (recursive) prefix
/// successful counterparts.
/// In this test reverted transactions are valid function calls that got insufficient steps limit.
#[case::v1(TransactionVersion::ONE, default_all_resource_bounds())]
#[case::l1_bounds(TransactionVersion::THREE, default_l1_resource_bounds())]
#[case::all_bounds(TransactionVersion::THREE, default_all_resource_bounds())]
fn test_reverted_reach_computation_limit(
    max_fee: Fee,
    mut block_context: BlockContext,
    #[case] version: TransactionVersion,
    #[case] resource_bounds: ValidResourceBounds,
    cairo_version: CairoVersion,
) {
    let TestInitData { mut state, account_address, contract_address, mut nonce_manager } =
        create_test_init_data(&block_context.chain_info, cairo_version);
    let tracked_resource = CompilerBasedVersion::CairoVersion(cairo_version).own_tracked_resource();

    // Limit the max amount of computation units (so we quickly hit the limit).
    block_context.versioned_constants.invoke_tx_max_n_steps = 6000;
    block_context.set_sierra_gas_limits(Some(GasAmount(600000)), None);
    let recursion_base_args = invoke_tx_args! {
        max_fee,
        resource_bounds,
        sender_address: account_address,
        version,
    };

    // Invoke the `recurse` function with 0 iterations. This call should succeed.
    let result = run_invoke_tx(
        &mut state,
        &block_context,
        invoke_tx_args! {
            nonce: nonce_manager.next(account_address),
            calldata: recursive_function_calldata(&contract_address, 0, false),
            ..recursion_base_args.clone()
        },
    )
    .unwrap();
    let n_units_0 =
        result.receipt.resources.computation.total_charged_computation_units(tracked_resource);
    let actual_fee_0 = result.receipt.fee.0;

    // Ensure the transaction was not reverted.
    assert!(!result.is_reverted());

    // Invoke the `recurse` function with 1 iteration. This call should succeed.
    let result = run_invoke_tx(
        &mut state,
        &block_context,
        invoke_tx_args! {
            nonce: nonce_manager.next(account_address),
            calldata: recursive_function_calldata(&contract_address, 1, false),
            ..recursion_base_args.clone()
        },
    )
    .unwrap();
    let n_units_1 =
        result.receipt.resources.computation.total_charged_computation_units(tracked_resource);
    let actual_fee_1 = result.receipt.fee.0;
    // Ensure the transaction was not reverted.
    assert!(!result.is_reverted());

    // Make sure that the n_units and actual_fee are higher as the recursion depth increases.
    let validate_tracked_resource = result.validate_call_info.unwrap().tracked_resource;
    assert_eq!(tracked_resource, validate_tracked_resource);
    assert!(n_units_1 > n_units_0);
    assert!(actual_fee_1 > actual_fee_0);

    // Calculate a recursion depth where the transaction will surely fail (not a minimal depth, as
    // base costs are neglected here).
    let units_diff = n_units_1 - n_units_0;
    // TODO(Ori, 1/2/2024): Write an indicative expect message explaining why the conversion works.
    let units_diff_as_u32: u32 = units_diff.try_into().expect("Failed to convert usize to u32.");
    let fail_depth = match tracked_resource {
        TrackedResource::CairoSteps => {
            block_context.versioned_constants.invoke_tx_max_n_steps / units_diff_as_u32
        }
        TrackedResource::SierraGas => {
            u32::try_from(block_context.versioned_constants.os_constants.execute_max_sierra_gas.0)
                .unwrap()
                / units_diff_as_u32
        }
    };

    // Invoke the `recurse` function with `fail_depth` iterations. This call should fail.
    let result = run_invoke_tx(
        &mut state,
        &block_context,
        invoke_tx_args! {
            nonce: nonce_manager.next(account_address),
            calldata: recursive_function_calldata(&contract_address, fail_depth, false),
            ..recursion_base_args.clone()
        },
    )
    .unwrap();
    let n_units_fail =
        result.receipt.resources.computation.total_charged_computation_units(tracked_resource);
    let actual_fee_fail: u128 = result.receipt.fee.0;
    // Ensure the transaction was reverted.
    assert!(result.is_reverted());

    // Make sure that the failed transaction gets charged for the extra computation, compared with
    // the smaller valid transaction.

    // If this assert fails, it may be due to the max computation limit being too low - i.e. this
    // new failure cannot run long enough to consume more computation units than the "1" case.
    // If this is the issue, try to increase the max computation limit on the block context above.
    assert!(n_units_fail > n_units_1);
    assert!(actual_fee_fail > actual_fee_1);

    // Invoke the `recurse` function with `fail_depth`+1 iterations. This call should fail.
    let result = run_invoke_tx(
        &mut state,
        &block_context,
        invoke_tx_args! {
            nonce: nonce_manager.next(account_address),
            calldata: recursive_function_calldata(&contract_address, fail_depth + 1, false),
            ..recursion_base_args
        },
    )
    .unwrap();
    let n_units_fail_next =
        result.receipt.resources.computation.total_charged_computation_units(tracked_resource);
    let actual_fee_fail_next: u128 = result.receipt.fee.0;
    // Ensure the transaction was reverted.
    assert!(result.is_reverted());

    // Test that the two reverted transactions behave the same.
    assert_eq!(n_units_fail, n_units_fail_next);
    assert_eq!(actual_fee_fail, actual_fee_fail_next);
}

#[rstest]
#[case::cairo0(CompilerBasedVersion::CairoVersion(CairoVersion::Cairo0))]
#[case::cairo1(CompilerBasedVersion::CairoVersion(CairoVersion::Cairo1(RunnableCairo1::Casm)))]
#[cfg_attr(
    feature = "cairo_native",
    case::cairo1_native(CompilerBasedVersion::CairoVersion(CairoVersion::Cairo1(
        RunnableCairo1::Native
    )))
)]
/// Tests that n_steps / n_reverted_gas and actual_fees of reverted transactions invocations are
/// consistent. In this test reverted transactions are recursive function invocations where the
/// innermost call asserts false. We test deltas between consecutive depths, and further depths.
fn test_n_reverted_computation_units(
    block_context: BlockContext,
    #[values(default_l1_resource_bounds(), default_all_resource_bounds())]
    resource_bounds: ValidResourceBounds,
    #[case] cairo_version: CompilerBasedVersion,
) {
    let TestInitData { mut state, account_address, contract_address, mut nonce_manager } =
        create_test_init_data(&block_context.chain_info, cairo_version.into());
    let recursion_base_args = invoke_tx_args! {
        resource_bounds,
        sender_address: account_address,
    };
    let tracked_resource = cairo_version.own_tracked_resource();

    // Invoke the `recursive_fail` function with 0 iterations. This call should fail.
    let result = run_invoke_tx(
        &mut state,
        &block_context,
        invoke_tx_args! {
            nonce: nonce_manager.next(account_address),
            calldata: recursive_function_calldata(&contract_address, 0, true),
            ..recursion_base_args.clone()
        },
    )
    .unwrap();
    // Ensure the transaction was reverted.
    assert!(result.is_reverted());
    let mut actual_resources_0 = result.receipt.resources.computation.clone();
    let n_units_0 =
        result.receipt.resources.computation.total_charged_computation_units(tracked_resource);
    let actual_fee_0 = result.receipt.fee.0;

    // Invoke the `recursive_fail` function with 1 iterations. This call should fail.
    let result = run_invoke_tx(
        &mut state,
        &block_context,
        invoke_tx_args! {
            nonce: nonce_manager.next(account_address),
            calldata: recursive_function_calldata(&contract_address, 1, true),
            ..recursion_base_args.clone()
        },
    )
    .unwrap();
    // Ensure the transaction was reverted.
    assert!(result.is_reverted());
    let actual_resources_1 = result.receipt.resources.computation;
    let n_units_1 = actual_resources_1.total_charged_computation_units(tracked_resource);
    let actual_fee_1 = result.receipt.fee.0;

    // Invoke the `recursive_fail` function with 2 iterations. This call should fail.
    let result = run_invoke_tx(
        &mut state,
        &block_context,
        invoke_tx_args! {
            nonce: nonce_manager.next(account_address),
            calldata: recursive_function_calldata(&contract_address, 2, true),
            ..recursion_base_args.clone()
        },
    )
    .unwrap();
    let n_units_2 =
        result.receipt.resources.computation.total_charged_computation_units(tracked_resource);
    let actual_fee_2 = result.receipt.fee.0;
    // Ensure the transaction was reverted.
    assert!(result.is_reverted());

    // Make sure that n_units and actual_fee diffs are the same for two consecutive reverted calls.
    assert_eq!(n_units_1 - n_units_0, n_units_2 - n_units_1);
    match (tracked_resource, resource_bounds.get_gas_vector_computation_mode()) {
        // If we are tracking sierra gas, but the user signed on L1 gas bounds only, we may have
        // rounding differences in the number of L1 gas units the user is charged for.
        // This rounding error can result in at most one unit of L1 gas difference, so the final fee
        // can differ by at most the L1 gas price.
        (TrackedResource::SierraGas, GasVectorComputationMode::NoL2Gas) => {
            assert!(
                (i128::try_from(actual_fee_1 - actual_fee_0).unwrap()
                    - i128::try_from(actual_fee_2 - actual_fee_1).unwrap())
                .unsigned_abs()
                    <= block_context.block_info.gas_prices.l1_gas_price(&FeeType::Strk).get().0
            );
        }
        _ => {
            assert_eq!(actual_fee_1 - actual_fee_0, actual_fee_2 - actual_fee_1);
        }
    }

    // Save the delta between two consecutive calls to be tested against a much larger recursion.
    let single_call_units_delta = n_units_1 - n_units_0;
    let single_call_fee_delta = actual_fee_1 - actual_fee_0;
    assert!(single_call_units_delta > 0);
    assert!(single_call_fee_delta > 0);

    // Make sure the resources in block of invocation 0 and 1 are the same, except for tracked
    // resource count.
    match tracked_resource {
        TrackedResource::CairoSteps => {
            actual_resources_0.n_reverted_steps += single_call_units_delta;
            assert_eq!(actual_resources_0, actual_resources_1);
        }
        TrackedResource::SierraGas => {
            actual_resources_0.reverted_sierra_gas.0 +=
                u64::try_from(single_call_units_delta).unwrap();
            assert_eq!(actual_resources_0, actual_resources_1);
        }
    };

    // Invoke the `recursive_fail` function with 100 iterations. This call should fail.
    let result = run_invoke_tx(
        &mut state,
        &block_context,
        invoke_tx_args! {
            nonce: nonce_manager.next(account_address),
            calldata: recursive_function_calldata(&contract_address, 100, true),
            ..recursion_base_args
        },
    )
    .unwrap();
    let n_units_100 =
        result.receipt.resources.computation.total_charged_computation_units(tracked_resource);
    let actual_fee_100 = result.receipt.fee.0;
    // Ensure the transaction was reverted.
    assert!(result.is_reverted());

    // Make sure that n_units and actual_fee grew as expected.
    assert_eq!(n_units_100 - n_units_0, 100 * single_call_units_delta);
    // When charging for sierra gas with no L2 gas user bound, due to rounding errors in converting
    // L2 gas units to L1 gas, the fee may be a bit off. We may have an error of one L1 gas units
    // per recursion, so the fee diff is at most 100 times the price of an L1 gas unit.
    match (tracked_resource, resource_bounds.get_gas_vector_computation_mode()) {
        (TrackedResource::SierraGas, GasVectorComputationMode::NoL2Gas) => {
            assert!(
                (i128::try_from(100 * single_call_fee_delta).unwrap()
                    - i128::try_from(actual_fee_100 - actual_fee_0).unwrap())
                .unsigned_abs()
                    < 100
                        * block_context.block_info.gas_prices.l1_gas_price(&FeeType::Strk).get().0
            );
        }
        _ => {
            assert_eq!(actual_fee_100 - actual_fee_0, 100 * single_call_fee_delta);
        }
    }
}

#[rstest]
fn test_max_fee_computation_from_tx_bounds(block_context: BlockContext) {
    macro_rules! assert_max_steps_as_expected {
        ($account_tx:expr, $expected_max_steps:expr $(,)?) => {
            let tx_context = Arc::new(block_context.to_tx_context(&$account_tx));
            let execution_context = EntryPointExecutionContext::new_invoke(
                tx_context,
                true,
                SierraGasRevertTracker::new(GasAmount::MAX),
            );
            let max_steps = execution_context.vm_run_resources.get_n_steps().unwrap();
            assert_eq!(u64::try_from(max_steps).unwrap(), $expected_max_steps);
        };
    }

    // V1 transaction: limit based on max fee.
    // Convert max fee to L1 gas units, and then to steps.
    let max_fee = Fee(100);
    let account_tx_max_fee = invoke_tx_with_default_flags(invoke_tx_args! {
        max_fee, version: TransactionVersion::ONE
    });
    let steps_per_l1_gas = block_context.versioned_constants.vm_resource_fee_cost().n_steps.inv();
    assert_max_steps_as_expected!(
        account_tx_max_fee,
        (steps_per_l1_gas
            * max_fee
                .checked_div(block_context.block_info.gas_prices.eth_gas_prices.l1_gas_price,)
                .unwrap()
                .0)
            .to_integer(),
    );

    // V3 transaction: limit based on L1 gas bounds.
    // Convert L1 gas units to steps.
    let l1_gas_bound = 200_u64;
    let account_tx_l1_bounds = invoke_tx_with_default_flags(invoke_tx_args! {
        resource_bounds: l1_resource_bounds(l1_gas_bound.into(), 1_u8.into()),
        version: TransactionVersion::THREE
    });
    assert_max_steps_as_expected!(
        account_tx_l1_bounds,
        (steps_per_l1_gas * l1_gas_bound).to_integer(),
    );

    // V3 transaction: limit based on L2 gas bounds (all resource_bounds).
    // Convert L2 gas units to steps.
    let l2_gas_bound = 300_u64;
    let account_tx_l2_bounds = invoke_tx_with_default_flags(invoke_tx_args! {
        resource_bounds: ValidResourceBounds::AllResources(AllResourceBounds {
            l2_gas: ResourceBounds {
                max_amount: l2_gas_bound.into(),
                max_price_per_unit: DEFAULT_STRK_L2_GAS_PRICE.into(),
            },
            ..Default::default()
        }),
        version: TransactionVersion::THREE
    });
    assert_max_steps_as_expected!(
        account_tx_l2_bounds,
        l2_gas_bound / block_context.versioned_constants.os_constants.gas_costs.base.step_gas_cost,
    );
}

#[rstest]
/// Tests that steps are correctly limited based on max_fee.
#[case(TransactionVersion::ONE)]
#[case(TransactionVersion::THREE)]
fn test_max_fee_to_max_steps_conversion(
    block_context: BlockContext,
    #[case] version: TransactionVersion,
) {
    let TestInitData { mut state, account_address, contract_address, mut nonce_manager } =
        create_test_init_data(&block_context.chain_info, CairoVersion::Cairo0);
    let actual_gas_used: GasAmount = u64_from_usize(
        get_const_syscall_resources(SyscallSelector::CallContract).n_steps
            + get_tx_resources(TransactionType::InvokeFunction).n_steps
            + 1743,
    )
    .into();
    let actual_fee = u128::from(actual_gas_used.0) * 100000000000;
    let actual_strk_gas_price = block_context.block_info.gas_prices.strk_gas_prices.l1_gas_price;
    let execute_calldata = create_calldata(
        contract_address,
        "with_arg",
        &[felt!(25_u8)], // Calldata: arg.
    );

    // First invocation of `with_arg` gets the exact pre-calculated actual fee as max_fee.
    let account_tx1 = invoke_tx_with_default_flags(invoke_tx_args! {
        max_fee: Fee(actual_fee),
        sender_address: account_address,
        calldata: execute_calldata.clone(),
        version,
        resource_bounds: l1_resource_bounds(actual_gas_used, actual_strk_gas_price.into()),
        nonce: nonce_manager.next(account_address),
    });
    let tx_context1 = Arc::new(block_context.to_tx_context(&account_tx1));
    let execution_context1 = EntryPointExecutionContext::new_invoke(
        tx_context1,
        true,
        SierraGasRevertTracker::new(GasAmount::MAX),
    );
    let max_steps_limit1 = execution_context1.vm_run_resources.get_n_steps();
    let tx_execution_info1 = account_tx1.execute(&mut state, &block_context).unwrap();
    let n_steps1 = tx_execution_info1.receipt.resources.computation.vm_resources.n_steps;
    let gas_used_vector1 = tx_execution_info1.receipt.resources.to_gas_vector(
        &block_context.versioned_constants,
        block_context.block_info.use_kzg_da,
        &GasVectorComputationMode::NoL2Gas,
    );

    // Second invocation of `with_arg` gets twice the pre-calculated actual fee as max_fee.
    let account_tx2 = invoke_tx_with_default_flags(invoke_tx_args! {
        max_fee: Fee(2 * actual_fee),
        sender_address: account_address,
        calldata: execute_calldata,
        version,
        resource_bounds:
            l1_resource_bounds((2 * actual_gas_used.0).into(), actual_strk_gas_price.into()),
        nonce: nonce_manager.next(account_address),
    });
    let tx_context2 = Arc::new(block_context.to_tx_context(&account_tx2));
    let execution_context2 = EntryPointExecutionContext::new_invoke(
        tx_context2,
        true,
        SierraGasRevertTracker::new(GasAmount::MAX),
    );
    let max_steps_limit2 = execution_context2.vm_run_resources.get_n_steps();
    let tx_execution_info2 = account_tx2.execute(&mut state, &block_context).unwrap();
    let n_steps2 = tx_execution_info2.receipt.resources.computation.vm_resources.n_steps;
    let gas_used_vector2 = tx_execution_info2.receipt.resources.to_gas_vector(
        &block_context.versioned_constants,
        block_context.block_info.use_kzg_da,
        &GasVectorComputationMode::NoL2Gas,
    );

    // Test that steps limit doubles as max_fee doubles, but actual consumed steps and fee remains.
    assert_eq!(max_steps_limit2.unwrap(), 2 * max_steps_limit1.unwrap());
    assert_eq!(tx_execution_info1.receipt.fee.0, tx_execution_info2.receipt.fee.0);
    // TODO(Ori, 1/2/2024): Write an indicative expect message explaining why the conversion works.
    // TODO(Aner, 21/01/24): verify test compliant with 4844 (or modify accordingly).
    assert_eq!(actual_gas_used, gas_used_vector2.l1_gas);
    assert_eq!(actual_fee, tx_execution_info2.receipt.fee.0);
    assert_eq!(n_steps1, n_steps2);
    assert_eq!(gas_used_vector1, gas_used_vector2);
}

#[rstest]
/// Tests that transactions with insufficient max_fee are reverted, the correct revert_error is
/// recorded and max_fee is charged.
fn test_insufficient_max_fee_reverts(
    block_context: BlockContext,
    #[values(default_l1_resource_bounds(), default_all_resource_bounds())]
    resource_bounds: ValidResourceBounds,
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1(RunnableCairo1::Casm))]
    cairo_version: CairoVersion,
) {
    let gas_mode = resource_bounds.get_gas_vector_computation_mode();
    let TestInitData { mut state, account_address, contract_address, mut nonce_manager } =
        create_test_init_data(&block_context.chain_info, cairo_version);
    let recursion_base_args = invoke_tx_args! {
        sender_address: account_address,
    };

    // Invoke the `recurse` function with depth 1 and MAX_FEE. This call should succeed.
    let tx_execution_info1 = run_invoke_tx(
        &mut state,
        &block_context,
        invoke_tx_args! {
            resource_bounds,
            nonce: nonce_manager.next(account_address),
            calldata: recursive_function_calldata(&contract_address, 1, false),
            ..recursion_base_args.clone()
        },
    )
    .unwrap();
    assert!(!tx_execution_info1.is_reverted());

    // Invoke the `recurse` function with depth of 2 and the actual gas usage of depth 1 as the
    // resource bounds. This call should fail in post-execution due to insufficient max gas (steps
    // bound based on the resource bounds are not so tight as to stop execution between iterations 1
    // and 2).
    let resource_used_depth1 = match gas_mode {
        GasVectorComputationMode::NoL2Gas => l1_resource_bounds(
            tx_execution_info1.receipt.gas.l1_gas,
            block_context.block_info.gas_prices.strk_gas_prices.l1_gas_price.into(),
        ),
        GasVectorComputationMode::All => ValidResourceBounds::all_bounds_from_vectors(
            &tx_execution_info1.receipt.gas,
            &block_context.block_info.gas_prices.strk_gas_prices,
        ),
    };
    let tx_execution_info2 = run_invoke_tx(
        &mut state,
        &block_context,
        invoke_tx_args! {
            resource_bounds: resource_used_depth1,
            nonce: nonce_manager.next(account_address),
            calldata: recursive_function_calldata(&contract_address, 2, false),
            ..recursion_base_args.clone()
        },
    )
    .unwrap();
    // In the L1 gas bounds case, due to resource limit being estimated by steps, the execution
    // will not fail due to insufficient resources; there are not enough steps in execution to hit
    // the bound. Post-execution will fail due to insufficient max fee.
    // The expected revert fee is therefore the same as the original fee (snap to bounds on
    // post-execution error).
    let overdraft_resource = match gas_mode {
        GasVectorComputationMode::NoL2Gas => Resource::L1Gas,
        GasVectorComputationMode::All => Resource::L2Gas,
    };
    assert!(tx_execution_info2.is_reverted());
    // DA costs should be identical, regardless of bounds; as should the final fee (computed by
    // snapping to bounds).
    assert_eq!(tx_execution_info2.receipt.da_gas, tx_execution_info1.receipt.da_gas);
    assert_eq!(tx_execution_info2.receipt.fee, tx_execution_info1.receipt.fee);
    assert!(
        tx_execution_info2
            .revert_error
            .unwrap()
            .to_string()
            .contains(&format!("Insufficient max {overdraft_resource}"))
    );

    // Invoke the `recurse` function with depth of 824 and the actual fee of depth 1 as max_fee.
    // This call should fail due to no remaining steps (execution steps based on max_fee are bounded
    // well enough to catch this mid-execution).
    let tx_execution_info3 = run_invoke_tx(
        &mut state,
        &block_context,
        invoke_tx_args! {
            resource_bounds: resource_used_depth1,
            nonce: nonce_manager.next(account_address),
            calldata: recursive_function_calldata(&contract_address, 824, false),
            ..recursion_base_args
        },
    )
    .unwrap();
    assert!(tx_execution_info3.is_reverted());
    match (cairo_version, resource_used_depth1) {
        (CairoVersion::Cairo0, _) => {
            assert_eq!(tx_execution_info3.receipt.fee, tx_execution_info1.receipt.fee);
            assert!(
                tx_execution_info3.revert_error.unwrap().to_string().contains("no remaining steps")
            );
        }
        (CairoVersion::Cairo1(RunnableCairo1::Casm), ValidResourceBounds::AllResources(_)) => {
            // There is no guarantee that out of gas errors result in maximal fee charge (as in - we
            // do not always expect the max amount of L2 gas to be charged). Assert final fee is
            // within a sane range.
            assert!(tx_execution_info3.revert_error.unwrap().to_string().contains("Out of gas"));
            assert!(Fee(2 * tx_execution_info3.receipt.fee.0) >= tx_execution_info1.receipt.fee);
            assert!(tx_execution_info3.receipt.fee <= Fee(2 * tx_execution_info1.receipt.fee.0));
        }
        (CairoVersion::Cairo1(RunnableCairo1::Casm), ValidResourceBounds::L1Gas(l1_bound)) => {
            // If a user supplied L1 gas bounds only, sierra gas is not limited in proportion to the
            // user bound; so we expect to revert in post-execution only.
            assert!(
                tx_execution_info3
                    .revert_error
                    .unwrap()
                    .to_string()
                    .contains("Insufficient max L1Gas")
            );
            assert_eq!(
                tx_execution_info3.receipt.fee,
                l1_bound
                    .max_amount
                    .checked_mul(
                        block_context.block_info.gas_prices.l1_gas_price(&FeeType::Strk).into()
                    )
                    .unwrap()
            );
        }
        #[cfg(feature = "cairo_native")]
        (CairoVersion::Cairo1(RunnableCairo1::Native), _) => {
            panic!("Test not implemented for cairo native.")
        }
    }
    assert_eq!(tx_execution_info3.receipt.da_gas, tx_execution_info1.receipt.da_gas);
}

#[rstest]
#[case::cairo0(CairoVersion::Cairo0)]
#[case::cairo1(CairoVersion::Cairo1(RunnableCairo1::Casm))]
#[cfg_attr(
    feature = "cairo_native",
    case::cairo1_native(CairoVersion::Cairo1(RunnableCairo1::Native))
)]
fn test_deploy_account_constructor_storage_write(
    default_all_resource_bounds: ValidResourceBounds,
    block_context: BlockContext,
    #[case] cairo_version: CairoVersion,
) {
    let grindy_account = FeatureContract::AccountWithLongValidate(cairo_version);
    let class_hash = grindy_account.get_class_hash();
    let chain_info = &block_context.chain_info;
    let state = &mut test_state(chain_info, BALANCE, &[(grindy_account, 1)]);

    let ctor_storage_arg = felt!(1_u8);
    let ctor_grind_arg = felt!(0_u8); // Do not grind in deploy phase.
    let constructor_calldata = calldata![ctor_grind_arg, ctor_storage_arg];
    let (deploy_account_tx, _) = deploy_and_fund_account(
        state,
        &mut NonceManager::default(),
        chain_info,
        deploy_account_tx_args! {
            class_hash,
            resource_bounds: default_all_resource_bounds,
            constructor_calldata: constructor_calldata.clone(),
        },
    );
    deploy_account_tx.execute(state, &block_context).unwrap();

    // Check that the constructor wrote ctor_arg to the storage.
    let storage_key = get_storage_var_address("ctor_arg", &[]);
    let deployed_contract_address = calculate_contract_address(
        ContractAddressSalt::default(),
        class_hash,
        &constructor_calldata,
        ContractAddress::default(),
    )
    .unwrap();
    let read_storage_arg = state.get_storage_at(deployed_contract_address, storage_key).unwrap();
    assert_eq!(ctor_storage_arg, read_storage_arg);
}

/// Test for counting actual storage changes.
#[apply(cairo_version)]
#[case::tx_version_1(TransactionVersion::ONE, FeeType::Eth)]
#[case::tx_version_3(TransactionVersion::THREE, FeeType::Strk)]
fn test_count_actual_storage_changes(
    max_fee: Fee,
    block_context: BlockContext,
    default_all_resource_bounds: ValidResourceBounds,
    #[case] version: TransactionVersion,
    #[case] fee_type: FeeType,
    cairo_version: CairoVersion,
) {
    // FeeType according to version.

    let chain_info = &block_context.chain_info;
    let fee_token_address = chain_info.fee_token_address(&fee_type);

    // Create initial state
    let TestInitData { mut state, account_address, contract_address, mut nonce_manager } =
        create_test_init_data(&block_context.chain_info, cairo_version);

    let sequencer_address = block_context.block_info.sequencer_address;
    let initial_sequencer_balance =
        state.get_fee_token_balance(sequencer_address, fee_token_address).unwrap().0;

    // Fee token var address.
    let sequencer_fee_token_var_address = get_fee_token_var_address(sequencer_address);
    let account_fee_token_var_address = get_fee_token_var_address(account_address);

    // Calldata types.
    let write_1_calldata =
        create_calldata(contract_address, "test_count_actual_storage_changes", &[]);
    let recipient = 435_u16;
    let transfer_amount: Felt = 1.into();
    let transfer_calldata = create_calldata(
        fee_token_address,
        TRANSFER_ENTRY_POINT_NAME,
        &[felt!(recipient), transfer_amount, felt!(0_u8)],
    );

    // Run transactions; using transactional state to count only storage changes of the current
    // transaction.
    // First transaction: storage cell value changes from 0 to 1.
    let mut state = TransactionalState::create_transactional(&mut state);
    let invoke_args = invoke_tx_args! {
        max_fee,
        resource_bounds: default_all_resource_bounds,
        version,
        sender_address: account_address,
        calldata: write_1_calldata,
        nonce: nonce_manager.next(account_address),
    };
    let account_tx = invoke_tx_with_default_flags(invoke_args.clone());
    let concurrency_mode = false;
    let execution_info =
        account_tx.execute_raw(&mut state, &block_context, concurrency_mode).unwrap();

    let fee_1 = execution_info.receipt.fee;
    let state_changes_1 = state.to_state_diff().unwrap();

    let cell_write_storage_change = ((contract_address, storage_key!(15_u8)), felt!(1_u8));
    let mut expected_sequencer_total_fee = initial_sequencer_balance + Felt::from(fee_1.0);
    let mut expected_sequencer_fee_update =
        ((fee_token_address, sequencer_fee_token_var_address), expected_sequencer_total_fee);
    let mut account_balance = BALANCE.0 - fee_1.0;
    let account_balance_storage_change =
        ((fee_token_address, account_fee_token_var_address), felt!(account_balance));

    let expected_modified_contracts =
        HashSet::from([account_address, contract_address, fee_token_address]);
    let expected_storage_updates_1 = HashMap::from([
        cell_write_storage_change,
        account_balance_storage_change,
        expected_sequencer_fee_update,
    ]);

    let state_changes_count_1 =
        state_changes_1.clone().count_for_fee_charge(Some(account_address), fee_token_address);
    let expected_state_changes_count_1 = StateChangesCountForFee {
        state_changes_count: StateChangesCount {
            // See expected storage updates.
            n_storage_updates: 3,
            // The contract address (storage update) and the account address (nonce update). Does
            // not include the fee token address as a modified contract.
            n_modified_contracts: 2,
            ..Default::default()
        },
        // Storage writing and sequencer fee update. The account balance storage change is not
        // allocated in this transaction.
        n_allocated_keys: 2,
    };

    assert_eq!(expected_modified_contracts, state_changes_1.state_maps.get_contract_addresses());
    assert_eq!(expected_storage_updates_1, state_changes_1.state_maps.storage);
    assert_eq!(state_changes_count_1, expected_state_changes_count_1);

    // Second transaction: storage cell starts and ends with value 1.
    let mut state = TransactionalState::create_transactional(&mut state);
    let account_tx = invoke_tx_with_default_flags(InvokeTxArgs {
        nonce: nonce_manager.next(account_address),
        ..invoke_args.clone()
    });
    let execution_info =
        account_tx.execute_raw(&mut state, &block_context, concurrency_mode).unwrap();

    let fee_2 = execution_info.receipt.fee;
    let state_changes_2 = state.to_state_diff().unwrap();

    expected_sequencer_total_fee += Felt::from(fee_2.0);
    expected_sequencer_fee_update.1 = expected_sequencer_total_fee;
    account_balance -= fee_2.0;
    let account_balance_storage_change =
        ((fee_token_address, account_fee_token_var_address), felt!(account_balance));

    let expected_modified_contracts_2 = HashSet::from([account_address, fee_token_address]);
    let expected_storage_updates_2 =
        HashMap::from([account_balance_storage_change, expected_sequencer_fee_update]);

    let state_changes_count_2 =
        state_changes_2.clone().count_for_fee_charge(Some(account_address), fee_token_address);
    let expected_state_changes_count_2 = StateChangesCountForFee {
        state_changes_count: StateChangesCount {
            // See expected storage updates.
            n_storage_updates: 2,
            // The account address (nonce update). Does not include the fee token address as a
            // modified contract.
            n_modified_contracts: 1,
            ..Default::default()
        },
        n_allocated_keys: 0,
    };

    assert_eq!(expected_modified_contracts_2, state_changes_2.state_maps.get_contract_addresses());
    assert_eq!(expected_storage_updates_2, state_changes_2.state_maps.storage);
    assert_eq!(state_changes_count_2, expected_state_changes_count_2);

    // Transfer transaction: transfer 1 ETH to recepient.
    let mut state = TransactionalState::create_transactional(&mut state);
    let account_tx = invoke_tx_with_default_flags(InvokeTxArgs {
        nonce: nonce_manager.next(account_address),
        calldata: transfer_calldata,
        ..invoke_args
    });
    let execution_info =
        account_tx.execute_raw(&mut state, &block_context, concurrency_mode).unwrap();

    let fee_transfer = execution_info.receipt.fee;
    let state_changes_transfer = state.to_state_diff().unwrap();
    let transfer_receipient_storage_change = (
        (fee_token_address, get_fee_token_var_address(contract_address!(recipient))),
        transfer_amount,
    );

    expected_sequencer_total_fee += Felt::from(fee_transfer.0);
    expected_sequencer_fee_update.1 = expected_sequencer_total_fee;
    account_balance -= fee_transfer.0 + 1; // Reduce the fee and the transfered amount (1).
    let account_balance_storage_change =
        ((fee_token_address, account_fee_token_var_address), felt!(account_balance));

    let expected_modified_contracts_transfer = HashSet::from([account_address, fee_token_address]);
    let expected_storage_update_transfer = HashMap::from([
        transfer_receipient_storage_change,
        account_balance_storage_change,
        expected_sequencer_fee_update,
    ]);

    let state_changes_count_3 = state_changes_transfer
        .clone()
        .count_for_fee_charge(Some(account_address), fee_token_address);
    let expected_state_changes_count_3 = StateChangesCountForFee {
        state_changes_count: StateChangesCount {
            // See expected storage updates.
            n_storage_updates: 3,
            // The account address (nonce update). Does not include the fee token address as a
            // modified contract.
            n_modified_contracts: 1,
            ..Default::default()
        },
        // A storage allocation for the recipient.
        n_allocated_keys: 1,
    };

    assert_eq!(
        expected_modified_contracts_transfer,
        state_changes_transfer.state_maps.get_contract_addresses()
    );
    assert_eq!(expected_storage_update_transfer, state_changes_transfer.state_maps.storage);
    assert_eq!(state_changes_count_3, expected_state_changes_count_3);
}

#[rstest]
#[case::tx_version_1(TransactionVersion::ONE, RunnableCairo1::Casm)]
#[case::tx_version_3(TransactionVersion::THREE, RunnableCairo1::Casm)]
#[cfg_attr(
    feature = "cairo_native",
    case::tx_version_3_native(TransactionVersion::THREE, RunnableCairo1::Native)
)]
fn test_concurrency_execute_fee_transfer(
    block_context: BlockContext,
    max_fee: Fee,
    default_all_resource_bounds: ValidResourceBounds,
    #[case] version: TransactionVersion,
    #[case] runnable: RunnableCairo1,
) {
    // TODO(Meshi, 01/06/2024): make the test so it will include changes in
    // sequencer_balance_key_high.
    const TRANSFER_AMOUNT: u128 = 100;
    const SEQUENCER_BALANCE_LOW_INITIAL: u128 = 50;
    let chain_info = &block_context.chain_info;
    let TestInitData { mut state, account_address, contract_address, .. } =
        create_test_init_data(chain_info, CairoVersion::Cairo1(runnable));
    let (sequencer_balance_key_low, sequencer_balance_key_high) =
        get_sequencer_balance_keys(&block_context);
    let account_tx = invoke_tx_with_default_flags(invoke_tx_args! {
    sender_address: account_address,
    max_fee,
    calldata: create_trivial_calldata(contract_address),
    resource_bounds: default_all_resource_bounds,
    version
    });
    let fee_type = &account_tx.fee_type();
    let fee_token_address = chain_info.fee_token_address(fee_type);

    // Case 1: The transaction did not read from/ write to the sequencer balance before executing
    // fee transfer.
    let mut transactional_state = TransactionalState::create_transactional(&mut state);
    let concurrency_mode = true;
    let result =
        account_tx.execute_raw(&mut transactional_state, &block_context, concurrency_mode).unwrap();
    assert!(!result.is_reverted());
    let transactional_cache = transactional_state.cache.borrow();
    for storage in [
        transactional_cache.initial_reads.storage.clone(),
        transactional_cache.writes.storage.clone(),
    ] {
        for seq_key in [sequencer_balance_key_low, sequencer_balance_key_high] {
            assert!(!storage.contains_key(&(fee_token_address, seq_key)));
        }
    }

    // Case 2: The transaction read from and write to the sequencer balance before executing fee
    // transfer.

    let transfer_calldata = create_calldata(
        fee_token_address,
        TRANSFER_ENTRY_POINT_NAME,
        &[*block_context.block_info.sequencer_address.0.key(), felt!(TRANSFER_AMOUNT), felt!(0_u8)],
    );

    // Set the sequencer balance to a constant value to check that the read set did not changed.
    fund_account(
        chain_info,
        block_context.block_info.sequencer_address,
        Fee(SEQUENCER_BALANCE_LOW_INITIAL),
        &mut state.state,
    );
    let mut transactional_state = TransactionalState::create_transactional(&mut state);

    // Invokes transfer to the sequencer.
    let account_tx = invoke_tx_with_default_flags(invoke_tx_args! {
        sender_address: account_address,
        calldata: transfer_calldata,
        max_fee,
        resource_bounds: default_all_resource_bounds,
    });

    let execution_result =
        account_tx.execute_raw(&mut transactional_state, &block_context, concurrency_mode);
    let result = execution_result.unwrap();
    assert!(!result.is_reverted());
    // Check that the sequencer balance was not updated.
    let storage_writes = transactional_state.cache.borrow().writes.storage.clone();
    let storage_initial_reads = transactional_state.cache.borrow().initial_reads.storage.clone();

    for (seq_write_val, expected_write_val) in [
        (
            storage_writes.get(&(fee_token_address, sequencer_balance_key_low)),
            // Balance after `execute` and without the fee transfer.
            felt!(SEQUENCER_BALANCE_LOW_INITIAL + TRANSFER_AMOUNT),
        ),
        (
            storage_initial_reads.get(&(fee_token_address, sequencer_balance_key_low)),
            felt!(SEQUENCER_BALANCE_LOW_INITIAL),
        ),
        (storage_writes.get(&(fee_token_address, sequencer_balance_key_high)), Felt::ZERO),
        (storage_initial_reads.get(&(fee_token_address, sequencer_balance_key_high)), Felt::ZERO),
    ] {
        assert_eq!(*seq_write_val.unwrap(), expected_write_val);
    }
}

// Check that when the sequencer is the sender, we run the sequential fee transfer.
#[rstest]
#[case::tx_version_1(TransactionVersion::ONE, RunnableCairo1::Casm)]
#[case::tx_version_3(TransactionVersion::THREE, RunnableCairo1::Casm)]
#[cfg_attr(
    feature = "cairo_native",
    case::tx_version_3_native(TransactionVersion::THREE, RunnableCairo1::Native)
)]
fn test_concurrent_fee_transfer_when_sender_is_sequencer(
    mut block_context: BlockContext,
    max_fee: Fee,
    default_all_resource_bounds: ValidResourceBounds,
    #[case] version: TransactionVersion,
    #[case] runnable: RunnableCairo1,
) {
    let chain_info = &block_context.chain_info;
    let TestInitData { mut state, account_address, contract_address, .. } =
        create_test_init_data(chain_info, CairoVersion::Cairo1(runnable));
    block_context.block_info.sequencer_address = account_address;
    let sender_balance = BALANCE;
    let (sequencer_balance_key_low, sequencer_balance_key_high) =
        get_sequencer_balance_keys(&block_context);
    let account_tx = invoke_tx_with_default_flags(invoke_tx_args! {
        max_fee,
        sender_address: account_address,
        calldata: create_trivial_calldata(contract_address),
        resource_bounds: default_all_resource_bounds,
        version
    });
    let fee_type = &account_tx.fee_type();
    let fee_token_address = chain_info.fee_token_address(fee_type);

    let mut transactional_state = TransactionalState::create_transactional(&mut state);
    let concurrency_mode = true;
    let result =
        account_tx.execute_raw(&mut transactional_state, &block_context, concurrency_mode).unwrap();
    assert!(!result.is_reverted());
    // Check that the sequencer balance was updated (in this case, was not changed).
    for (seq_key, seq_value) in
        [(sequencer_balance_key_low, sender_balance), (sequencer_balance_key_high, Fee(0))]
    {
        assert_eq!(state.get_storage_at(fee_token_address, seq_key).unwrap(), felt!(seq_value.0));
    }
}

/// Check initial gas is as expected according to the contract cairo+compiler version, and call
/// history.
#[rstest]
#[case::cairo0(&[
    CompilerBasedVersion::CairoVersion(CairoVersion::Cairo1(RunnableCairo1::Casm)),
    CompilerBasedVersion::CairoVersion(CairoVersion::Cairo1(RunnableCairo1::Casm)),
    CompilerBasedVersion::CairoVersion(CairoVersion::Cairo0),
    CompilerBasedVersion::CairoVersion(CairoVersion::Cairo1(RunnableCairo1::Casm))])]
#[case::old_cairo1(&[
    CompilerBasedVersion::CairoVersion(CairoVersion::Cairo1(RunnableCairo1::Casm)),
    CompilerBasedVersion::CairoVersion(CairoVersion::Cairo1(RunnableCairo1::Casm)),
    CompilerBasedVersion::OldCairo1,
    CompilerBasedVersion::CairoVersion(CairoVersion::Cairo1(RunnableCairo1::Casm))])]
fn test_initial_gas(
    block_context: BlockContext,
    #[case] versions: &[CompilerBasedVersion],
    default_all_resource_bounds: ValidResourceBounds,
) {
    let account_version = CairoVersion::Cairo1(RunnableCairo1::Casm);
    let account = FeatureContract::AccountWithoutValidations(account_version);
    let account_address = account.get_instance_address(0_u16);
    let used_test_contracts: HashSet<FeatureContract> =
        HashSet::from_iter(versions.iter().map(|x| x.get_test_contract()));
    let mut contracts: Vec<FeatureContract> = used_test_contracts.into_iter().collect();
    contracts.push(account);
    let sender_balance = BALANCE;
    let chain_info = &block_context.chain_info;
    let state = &mut test_state(
        chain_info,
        sender_balance,
        &contracts.into_iter().map(|contract| (contract, 1u16)).collect::<Vec<_>>(),
    );
    let account_tx = invoke_tx_with_default_flags(invoke_tx_args! {
        sender_address: account_address,
        calldata: build_recurse_calldata(versions),
        resource_bounds:  default_all_resource_bounds,
        version: TransactionVersion::THREE
    });
    let user_gas_bound = block_context.to_tx_context(&account_tx).initial_sierra_gas();

    let transaction_ex_info = account_tx.execute(state, &block_context).unwrap();

    let validate_call_info = &transaction_ex_info.validate_call_info.unwrap();
    let validate_initial_gas = validate_call_info.call.initial_gas;
    assert_eq!(
        validate_initial_gas,
        block_context.versioned_constants.os_constants.validate_max_sierra_gas.0
    );
    let validate_gas_consumed = validate_call_info.execution.gas_consumed;
    assert!(validate_gas_consumed > 0, "New Cairo1 contract should consume gas.");

    let default_call_info = CallInfo::default();
    let mut execute_call_info = &transaction_ex_info.execute_call_info.unwrap();
    // Initial gas for execution is the minimum between the max execution gas and the initial gas
    // minus the gas consumed by validate. Need to add 1 as the check is strictly less than.
    let mut prev_initial_gas = block_context
        .versioned_constants
        .os_constants
        .execute_max_sierra_gas
        .min(user_gas_bound - GasAmount(validate_gas_consumed) + GasAmount(1))
        .0;

    let mut curr_initial_gas;
    let mut started_vm_mode = false;
    // The __validate__ call of a the account contract.
    let mut prev_version = &CompilerBasedVersion::CairoVersion(account_version);
    // Insert the __execute__ call in the beginning of versions (same version as the __validate__).
    for version in [*prev_version].iter().chain(versions) {
        curr_initial_gas = execute_call_info.call.initial_gas;

        match (prev_version, version, started_vm_mode) {
            (
                CompilerBasedVersion::CairoVersion(CairoVersion::Cairo0)
                | CompilerBasedVersion::OldCairo1,
                _,
                _,
            ) => {
                assert_eq!(started_vm_mode, true);
                assert_eq!(curr_initial_gas, prev_initial_gas);
            }
            (
                _,
                CompilerBasedVersion::CairoVersion(CairoVersion::Cairo0)
                | CompilerBasedVersion::OldCairo1,
                false,
            ) => {
                // First time we are in VM mode.
                assert_matches!(
                    prev_version,
                    &CompilerBasedVersion::CairoVersion(CairoVersion::Cairo1(_))
                );
                assert_eq!(
                    curr_initial_gas,
                    block_context.versioned_constants.infinite_gas_for_vm_mode()
                );
                started_vm_mode = true;
            }
            _ => {
                // prev_version is a non CairoStep contract, thus it consumes gas from the initial
                // gas.
                assert!(curr_initial_gas < prev_initial_gas);
                if matches!(version, &CompilerBasedVersion::CairoVersion(CairoVersion::Cairo1(_))) {
                    assert!(execute_call_info.execution.gas_consumed > 0);
                } else {
                    assert!(execute_call_info.execution.gas_consumed == 0);
                }
            }
        };

        // If inner_calls is empty, this SHOULD be the last call and thus last loop iteration.
        // Assigning the default call info, will cause an error if the loop continues.
        assert!(execute_call_info.inner_calls.len() <= 1);
        execute_call_info = execute_call_info.inner_calls.first().unwrap_or(&default_call_info);
        prev_initial_gas = curr_initial_gas;
        prev_version = version;
    }
}

#[rstest]
#[case::cairo1(CairoVersion::Cairo1(RunnableCairo1::Casm))]
#[cfg_attr(
    feature = "cairo_native",
    case::cairo1_native(CairoVersion::Cairo1(RunnableCairo1::Native))
)]
fn test_revert_in_execute(
    block_context: BlockContext,
    default_all_resource_bounds: ValidResourceBounds,
    #[case] cairo_version: CairoVersion,
) {
    let TestInitData { mut state, account_address, mut nonce_manager, .. } =
        create_test_init_data(&block_context.chain_info, cairo_version);
    // Invoke a function that changes the state and reverts.
    let tx_args = invoke_tx_args! {
        sender_address: account_address,
        calldata: Calldata(vec![].into()),
        nonce: nonce_manager.next(account_address)
    };

    // Skip validate phase, as we want to test the revert in the execute phase.
    let validate = false;
    let tx = executable_invoke_tx(invoke_tx_args! {
        resource_bounds: default_all_resource_bounds,
        ..tx_args
    });
    let execution_flags = AccountExecutionFlags { validate, ..AccountExecutionFlags::default() };
    let account_tx = AccountTransaction { tx, execution_flags };
    let tx_execution_info = account_tx.execute(&mut state, &block_context).unwrap();

    assert!(tx_execution_info.is_reverted());
    assert!(
        tx_execution_info
            .revert_error
            .unwrap()
            .to_string()
            .contains("Failed to deserialize param #1")
    );
}

#[rstest]
#[cfg_attr(feature = "cairo_native", case::native(CairoVersion::Cairo1(RunnableCairo1::Native)))]
#[case::vm(CairoVersion::Cairo1(RunnableCairo1::Casm))]
fn test_call_contract_that_panics(
    #[case] cairo_version: CairoVersion,
    mut block_context: BlockContext,
    default_all_resource_bounds: ValidResourceBounds,
    #[values(true, false)] enable_reverts: bool,
    #[values("test_revert_helper", "bad_selector")] inner_selector: &str,
) {
    // Override enable reverts.
    block_context.versioned_constants.enable_reverts = enable_reverts;
    let TestInitData { mut state, account_address, contract_address, mut nonce_manager } =
        create_test_init_data(&block_context.chain_info, cairo_version);

    let new_class_hash = FeatureContract::TestContract(cairo_version).get_class_hash();
    let to_panic = true.into();

    let calldata = [
        *contract_address.0.key(),
        selector_from_name(inner_selector).0,
        felt!(2_u8),
        new_class_hash.0,
        to_panic,
    ];

    // Invoke a function that changes the state and reverts.
    let tx_args = invoke_tx_args! {
        sender_address: account_address,
        calldata: create_calldata(
            contract_address,
               "test_call_contract_revert",
                &calldata
            ),
        nonce: nonce_manager.next(account_address)
    };
    let tx_execution_info = run_invoke_tx(
        &mut state,
        &block_context,
        invoke_tx_args! {
            resource_bounds: default_all_resource_bounds,
            ..tx_args
        },
    )
    .unwrap();

    // If reverts are enabled, `test_call_contract_revert` should catch it and ignore it.
    // Otherwise, the transaction should revert.
    assert_eq!(tx_execution_info.is_reverted(), !enable_reverts);

    if enable_reverts {
        // Check that the tracked resource is SierraGas to make sure that Native is running.
        for call in tx_execution_info.execute_call_info.unwrap().iter() {
            assert_eq!(call.tracked_resource, TrackedResource::SierraGas);
        }
    }
}
