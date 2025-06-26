use assert_matches::assert_matches;
use blockifier_test_utils::cairo_versions::CairoVersion;
use blockifier_test_utils::calldata::{create_calldata, create_trivial_calldata};
use blockifier_test_utils::contracts::FeatureContract;
use pretty_assertions::assert_eq;
use rstest::rstest;
use starknet_api::block::FeeType;
use starknet_api::core::ContractAddress;
use starknet_api::executable_transaction::TransactionType;
use starknet_api::execution_resources::{GasAmount, GasVector};
use starknet_api::test_utils::invoke::{executable_invoke_tx, InvokeTxArgs};
use starknet_api::test_utils::{
    NonceManager,
    DEFAULT_L1_GAS_AMOUNT,
    DEFAULT_STRK_L1_GAS_PRICE,
    MAX_FEE,
};
use starknet_api::transaction::fields::{
    Calldata,
    Fee,
    GasVectorComputationMode,
    Resource,
    Tip,
    TransactionSignature,
    ValidResourceBounds,
};
use starknet_api::transaction::TransactionVersion;
use starknet_api::{felt, invoke_tx_args, nonce};
use starknet_types_core::felt::Felt;

use crate::blockifier_versioned_constants::AllocationCost;
use crate::context::{BlockContext, ChainInfo};
use crate::execution::syscalls::vm_syscall_utils::SyscallSelector;
use crate::fee::fee_utils::get_fee_by_gas_vector;
use crate::state::cached_state::CachedState;
use crate::state::state_api::StateReader;
use crate::test_utils::dict_state_reader::DictStateReader;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::{get_const_syscall_resources, get_tx_resources, BALANCE};
use crate::transaction::account_transaction::{AccountTransaction, ExecutionFlags};
use crate::transaction::errors::{
    ResourceBoundsError,
    TransactionExecutionError,
    TransactionFeeError,
    TransactionPreValidationError,
};
use crate::transaction::objects::{TransactionExecutionInfo, TransactionExecutionResult};
use crate::transaction::test_utils::{
    default_l1_resource_bounds,
    invoke_tx_with_default_flags,
    l1_resource_bounds,
    INVALID,
};
use crate::transaction::transactions::ExecutableTransaction;
use crate::utils::u64_from_usize;
const VALIDATE_GAS_OVERHEAD: GasAmount = GasAmount(21);

struct FlavorTestInitialState {
    pub state: CachedState<DictStateReader>,
    pub account_address: ContractAddress,
    pub faulty_account_address: ContractAddress,
    pub test_contract_address: ContractAddress,
    pub nonce_manager: NonceManager,
}

fn create_flavors_test_state(
    chain_info: &ChainInfo,
    cairo_version: CairoVersion,
) -> FlavorTestInitialState {
    let test_contract = FeatureContract::TestContract(cairo_version);
    let account_contract = FeatureContract::AccountWithoutValidations(cairo_version);
    let faulty_account_contract = FeatureContract::FaultyAccount(cairo_version);
    let state = test_state(
        chain_info,
        BALANCE,
        &[(account_contract, 1), (faulty_account_contract, 1), (test_contract, 1)],
    );
    FlavorTestInitialState {
        state,
        account_address: account_contract.get_instance_address(0),
        faulty_account_address: faulty_account_contract.get_instance_address(0),
        test_contract_address: test_contract.get_instance_address(0),
        nonce_manager: NonceManager::default(),
    }
}

/// Checks that balance of the account decreased if and only if `charge_fee` is true.
/// Returns the new balance.
fn check_balance<S: StateReader>(
    current_balance: Felt,
    state: &CachedState<S>,
    account_address: ContractAddress,
    chain_info: &ChainInfo,
    fee_type: &FeeType,
    charge_fee: bool,
) -> Felt {
    let (new_balance, _) = state
        .get_fee_token_balance(account_address, chain_info.fee_token_address(fee_type))
        .unwrap();
    if charge_fee {
        assert!(new_balance < current_balance);
    } else {
        assert_eq!(new_balance, current_balance);
    }
    new_balance
}

/// Returns the amount of L1 gas and derived fee, given base gas amount and a boolean indicating
/// if validation is to be done.
fn gas_and_fee(
    base_gas: GasAmount,
    add_validation_overhead: bool,
    fee_type: &FeeType,
) -> (GasAmount, Fee) {
    // Validation incurs a constant gas overhead.
    let gas = base_gas + if add_validation_overhead { VALIDATE_GAS_OVERHEAD } else { 0_u8.into() };
    (
        gas,
        get_fee_by_gas_vector(
            &BlockContext::create_for_account_testing().block_info,
            GasVector::from_l1_gas(gas),
            fee_type,
            Tip::ZERO,
        ),
    )
}

// Calculates the actual gas used by a transaction. Removing the validation overhead if requested,
// as it's already considered in the tx_execution_info.
fn calculate_actual_gas(
    tx_execution_info: &TransactionExecutionInfo,
    block_context: &BlockContext,
    remove_validation_overhead: bool,
) -> GasAmount {
    (tx_execution_info
        .receipt
        .resources
        .to_gas_vector(
            &block_context.versioned_constants,
            block_context.block_info.use_kzg_da,
            &GasVectorComputationMode::NoL2Gas,
        )
        .l1_gas
        .0
        - if remove_validation_overhead { VALIDATE_GAS_OVERHEAD.0 } else { 0 })
    .into()
}

/// Asserts gas used and reported fee are as expected.
fn check_gas_and_fee(
    block_context: &BlockContext,
    tx_execution_info: &TransactionExecutionInfo,
    fee_type: &FeeType,
    expected_actual_gas: GasAmount,
    expected_actual_fee: Fee,
    expected_cost_of_resources: Fee,
) {
    assert_eq!(calculate_actual_gas(tx_execution_info, block_context, false), expected_actual_gas);

    assert_eq!(tx_execution_info.receipt.fee, expected_actual_fee);
    // Future compatibility: resources other than the L1 gas usage may affect the fee. These tests
    // are not implemented for the AllBounds case.
    let no_l2_gas_vector = tx_execution_info.receipt.resources.to_gas_vector(
        &block_context.versioned_constants,
        block_context.block_info.use_kzg_da,
        &GasVectorComputationMode::NoL2Gas,
    );
    let no_l2_gas_fee =
        get_fee_by_gas_vector(&block_context.block_info, no_l2_gas_vector, fee_type, Tip::ZERO);

    assert_eq!(no_l2_gas_fee, expected_cost_of_resources);
}

fn recurse_calldata(contract_address: ContractAddress, fail: bool, depth: u32) -> Calldata {
    create_calldata(
        contract_address,
        if fail { "recursive_fail" } else { "recurse" },
        &[felt!(depth)],
    )
}

// Helper function to get the arguments for the pre-validation tests.
fn get_pre_validate_test_args(
    cairo_version: CairoVersion,
    version: TransactionVersion,
) -> (BlockContext, CachedState<DictStateReader>, InvokeTxArgs, NonceManager) {
    let block_context = BlockContext::create_for_account_testing();
    let max_fee = MAX_FEE;
    // The max resource bounds fixture is not used here because this function already has the
    // maximum number of arguments.
    let resource_bounds =
        l1_resource_bounds(DEFAULT_L1_GAS_AMOUNT, DEFAULT_STRK_L1_GAS_PRICE.into());
    let FlavorTestInitialState {
        state, account_address, test_contract_address, nonce_manager, ..
    } = create_flavors_test_state(&block_context.chain_info, cairo_version);

    let pre_validation_base_args = invoke_tx_args! {
        max_fee,
        resource_bounds,
        sender_address: account_address,
        calldata: create_trivial_calldata(test_contract_address),
        version,
    };
    (block_context, state, pre_validation_base_args, nonce_manager)
}

// A pre-validation scenario: Invalid nonce.
#[rstest]
fn test_invalid_nonce_pre_validate(
    #[values(true, false)] only_query: bool,
    #[values(true, false)] validate: bool,
    #[values(true, false)] charge_fee: bool,
    // TODO(Dori, 1/1/2024): Add Cairo1 case, after price abstraction is implemented.
    #[values(CairoVersion::Cairo0)] cairo_version: CairoVersion,
    #[values(TransactionVersion::ONE, TransactionVersion::THREE)] version: TransactionVersion,
) {
    let (block_context, mut state, pre_validation_base_args, _) =
        get_pre_validate_test_args(cairo_version, version);
    let account_address = pre_validation_base_args.sender_address;

    // First scenario: invalid nonce. Regardless of flags, should fail.
    let invalid_nonce = nonce!(7_u8);
    let account_nonce = state.get_nonce_at(account_address).unwrap();
    let tx =
        executable_invoke_tx(invoke_tx_args! {nonce: invalid_nonce, ..pre_validation_base_args});
    let execution_flags =
        ExecutionFlags { only_query, charge_fee, validate, strict_nonce_check: true };
    let account_tx = AccountTransaction { tx, execution_flags };
    let result = account_tx.execute(&mut state, &block_context);
    assert_matches!(
        result.unwrap_err(),
        TransactionExecutionError::TransactionPreValidationError(boxed_error)
        if matches!(
            *boxed_error,
            TransactionPreValidationError::InvalidNonce {
                address, account_nonce: expected_nonce, incoming_tx_nonce
            }
            if (address, expected_nonce, incoming_tx_nonce) ==
            (account_address, account_nonce, invalid_nonce)
        )
    );
}
// Pre-validation scenarios.
// 1. Not enough resource bounds for minimal fee.
// 2. Not enough balance for resource bounds.
// 3. Max L1 gas price is too low (non-deprecated transactions only).
// In all scenarios, no need for balance check - balance shouldn't change regardless of flags.

/// Test simulate / validate / charge_fee flag combinations in pre-validation stage.
#[rstest]
#[case(TransactionVersion::ONE, FeeType::Eth, true)]
#[case(TransactionVersion::THREE, FeeType::Strk, false)]
fn test_simulate_validate_pre_validate_with_charge_fee(
    #[values(true, false)] only_query: bool,
    #[values(true, false)] validate: bool,
    // TODO(Dori, 1/1/2024): Add Cairo1 case, after price abstraction is implemented.
    #[values(CairoVersion::Cairo0)] cairo_version: CairoVersion,
    #[case] version: TransactionVersion,
    #[case] fee_type: FeeType,
    #[case] is_deprecated: bool,
) {
    let charge_fee = true;
    let (block_context, mut state, pre_validation_base_args, mut nonce_manager) =
        get_pre_validate_test_args(cairo_version, version);
    let account_address = pre_validation_base_args.sender_address;

    // First scenario: minimal fee not covered. Actual fee is precomputed.
    let err = invoke_tx_with_default_flags(invoke_tx_args! {
        max_fee: Fee(10),
        resource_bounds: l1_resource_bounds(10_u8.into(), 10_u8.into()),
        nonce: nonce_manager.next(account_address),

        ..pre_validation_base_args.clone()
    })
    .execute(&mut state, &block_context)
    .unwrap_err();

    nonce_manager.rollback(account_address);
    if is_deprecated {
        assert_matches!(
            err,
            TransactionExecutionError::TransactionPreValidationError(boxed_error)
            if matches!(
                *boxed_error,
                TransactionPreValidationError::TransactionFeeError(
                    TransactionFeeError::MaxFeeTooLow { .. }
                )
            )
        );
    } else {
        assert_matches!(
            err,
            TransactionExecutionError::TransactionPreValidationError(boxed_error)
            => assert_matches!(
                *boxed_error,
                TransactionPreValidationError::TransactionFeeError(
                    TransactionFeeError::InsufficientResourceBounds { errors }
                )
                => assert_matches!(
                    errors[0],
                    ResourceBoundsError::MaxGasAmountTooLow { resource , .. }
                    if resource == Resource::L1Gas
                )
            )
        );
    }

    // Second scenario: resource bounds greater than balance.
    let gas_price = block_context.block_info.gas_prices.l1_gas_price(&fee_type);
    let balance_over_gas_price = BALANCE.checked_div(gas_price).unwrap();
    let tx = executable_invoke_tx(invoke_tx_args! {
        max_fee: Fee(BALANCE.0 + 1),
        resource_bounds: l1_resource_bounds(
            (balance_over_gas_price.0 + 10).into(),
            gas_price.into()
        ),
        nonce: nonce_manager.next(account_address),

        ..pre_validation_base_args.clone()
    });
    let account_tx = AccountTransaction {
        tx,
        execution_flags: ExecutionFlags {
            only_query,
            charge_fee,
            validate,
            strict_nonce_check: true,
        },
    };
    let result = account_tx.execute(&mut state, &block_context);

    nonce_manager.rollback(account_address);
    if is_deprecated {
        assert_matches!(
            result.unwrap_err(),
            TransactionExecutionError::TransactionPreValidationError(boxed_error)
            if matches!(
                *boxed_error,
                TransactionPreValidationError::TransactionFeeError(
                    TransactionFeeError::MaxFeeExceedsBalance { .. }
                )
            )
        );
    } else {
        assert_matches!(
            result.unwrap_err(),
            TransactionExecutionError::TransactionPreValidationError(boxed_error)
            if matches!(
                *boxed_error,
                TransactionPreValidationError::TransactionFeeError(
                    TransactionFeeError::GasBoundsExceedBalance {resource, .. }
                )
                if resource == Resource::L1Gas
            )
        );
    }

    // Third scenario: L1 gas price bound lower than the price on the block.
    if !is_deprecated {
        let tx = executable_invoke_tx(invoke_tx_args! {
            resource_bounds: l1_resource_bounds(DEFAULT_L1_GAS_AMOUNT, (gas_price.get().0 - 1).into()),
            nonce: nonce_manager.next(account_address),

            ..pre_validation_base_args
        });
        let account_tx = AccountTransaction {
            tx,
            execution_flags: ExecutionFlags {
                only_query,
                charge_fee,
                validate,
                ..Default::default()
            },
        };
        let err = account_tx.execute(&mut state, &block_context).unwrap_err();

        nonce_manager.rollback(account_address);
        assert_matches!(
            err,
            TransactionExecutionError::TransactionPreValidationError(boxed_error)
            => assert_matches!(
                *boxed_error,
                TransactionPreValidationError::TransactionFeeError(
                    TransactionFeeError::InsufficientResourceBounds{ errors }
                )
                => assert_matches!(
                    errors[0],
                    ResourceBoundsError::MaxGasPriceTooLow { resource, .. }
                    if resource == Resource::L1Gas
                )
            )
        );
    }
}

/// Test simulate / validate / charge_fee flag combinations in pre-validation stage.
#[rstest]
#[case(TransactionVersion::ONE, FeeType::Eth, true)]
#[case(TransactionVersion::THREE, FeeType::Strk, false)]
fn test_simulate_validate_pre_validate_not_charge_fee(
    #[values(true, false)] only_query: bool,
    #[values(true, false)] validate: bool,
    // TODO(Dori, 1/1/2024): Add Cairo1 case, after price abstraction is implemented.
    #[values(CairoVersion::Cairo0)] cairo_version: CairoVersion,
    #[case] version: TransactionVersion,
    #[case] fee_type: FeeType,
    #[case] is_deprecated: bool,
) {
    let charge_fee = false;
    let (block_context, mut state, pre_validation_base_args, mut nonce_manager) =
        get_pre_validate_test_args(cairo_version, version);
    let account_address = pre_validation_base_args.sender_address;

    let tx = executable_invoke_tx(invoke_tx_args! {
        nonce: nonce_manager.next(account_address),
        ..pre_validation_base_args.clone()
    });
    let account_tx = AccountTransaction {
        tx,
        execution_flags: ExecutionFlags {
            only_query,
            charge_fee,
            validate: false,
            ..Default::default()
        },
    };
    let tx_execution_info = account_tx.execute(&mut state, &block_context).unwrap();
    let base_gas = calculate_actual_gas(&tx_execution_info, &block_context, false);
    assert!(
        base_gas
            > u64_from_usize(
                get_const_syscall_resources(SyscallSelector::CallContract).n_steps
                    + get_tx_resources(TransactionType::InvokeFunction).n_steps
            )
            .into()
    );

    let (actual_gas_used, actual_fee) = gas_and_fee(base_gas, validate, &fee_type);
    macro_rules! execute_and_check_gas_and_fee {
        ($max_fee:expr, $resource_bounds:expr) => {{
            let tx = executable_invoke_tx(invoke_tx_args! {
                max_fee: $max_fee,
                resource_bounds: $resource_bounds,
                nonce: nonce_manager.next(account_address),

                ..pre_validation_base_args.clone()
            });
            let account_tx = AccountTransaction {
                tx,
                execution_flags: ExecutionFlags {
                    only_query,
                    charge_fee,
                    validate,
                    ..Default::default()
                },
            };
            let tx_execution_info = account_tx.execute(&mut state, &block_context).unwrap();
            check_gas_and_fee(
                &block_context,
                &tx_execution_info,
                &fee_type,
                actual_gas_used,
                actual_fee,
                actual_fee,
            );
        }};
    }

    // First scenario: minimal fee not covered. Actual fee is precomputed.
    execute_and_check_gas_and_fee!(Fee(10), l1_resource_bounds(10_u8.into(), 10_u8.into()));

    // Second scenario: resource bounds greater than balance.
    let gas_price = block_context.block_info.gas_prices.l1_gas_price(&fee_type);
    let balance_over_gas_price = BALANCE.checked_div(gas_price).unwrap();
    execute_and_check_gas_and_fee!(
        Fee(BALANCE.0 + 1),
        l1_resource_bounds((balance_over_gas_price.0 + 10).into(), gas_price.into())
    );

    // Third scenario: L1 gas price bound lower than the price on the block.
    if !is_deprecated {
        execute_and_check_gas_and_fee!(
            pre_validation_base_args.max_fee,
            l1_resource_bounds(DEFAULT_L1_GAS_AMOUNT, (gas_price.get().0 - 1).into())
        );
    }
}

// Helper function to execute a transaction that fails validation.
fn execute_fail_validation(
    only_query: bool,
    validate: bool,
    charge_fee: bool,
    cairo_version: CairoVersion,
    version: TransactionVersion,
    max_resource_bounds: ValidResourceBounds,
) -> TransactionExecutionResult<TransactionExecutionInfo> {
    let block_context = BlockContext::create_for_account_testing();
    let max_fee = MAX_FEE;

    // Create a state with a contract that can fail validation on demand.
    let FlavorTestInitialState {
        state: mut falliable_state,
        faulty_account_address,
        mut nonce_manager,
        ..
    } = create_flavors_test_state(&block_context.chain_info, cairo_version);

    // Validation scenario: fallible validation.
    let tx = executable_invoke_tx(invoke_tx_args! {
        max_fee,
        resource_bounds: max_resource_bounds,
        signature: TransactionSignature(vec![
            Felt::from(INVALID),
            Felt::ZERO
        ].into()),
        sender_address: faulty_account_address,
        calldata: create_calldata(faulty_account_address, "foo", &[]),
        version,
        nonce: nonce_manager.next(faulty_account_address),
    });
    let account_tx = AccountTransaction {
        tx,
        execution_flags: ExecutionFlags {
            only_query,
            charge_fee,
            validate,
            strict_nonce_check: true,
        },
    };
    account_tx.execute(&mut falliable_state, &block_context)
}

/// Test simulate / charge_fee flag combinations in (fallible) validation stage.
#[rstest]
fn test_simulate_charge_fee_with_validation_fail_validate(
    #[values(true, false)] only_query: bool,
    #[values(true, false)] charge_fee: bool,
    // TODO(Dori, 1/1/2024): Add Cairo1 case, after price abstraction is implemented.
    #[values(CairoVersion::Cairo0)] cairo_version: CairoVersion,
    #[values(TransactionVersion::ONE, TransactionVersion::THREE)] version: TransactionVersion,
    default_l1_resource_bounds: ValidResourceBounds,
) {
    let validate = true;
    assert!(
        execute_fail_validation(
            only_query,
            validate,
            charge_fee,
            cairo_version,
            version,
            default_l1_resource_bounds,
        )
        .unwrap_err()
        .to_string()
        .contains("An ASSERT_EQ instruction failed: 1 != 0.")
    );
}

/// Test gas and fee with simulate / charge_fee flag combinations in (fallible) validation stage,
/// where validation is disabled.
#[rstest]
#[case(TransactionVersion::ONE, FeeType::Eth)]
#[case(TransactionVersion::THREE, FeeType::Strk)]
fn test_simulate_charge_fee_no_validation_fail_validate(
    #[values(true, false)] only_query: bool,
    #[values(true, false)] charge_fee: bool,
    // TODO(Dori, 1/1/2024): Add Cairo1 case, after price abstraction is implemented.
    #[values(CairoVersion::Cairo0)] cairo_version: CairoVersion,
    #[case] version: TransactionVersion,
    #[case] fee_type: FeeType,
    default_l1_resource_bounds: ValidResourceBounds,
) {
    let validate = false;
    let tx_execution_info = execute_fail_validation(
        only_query,
        validate,
        charge_fee,
        cairo_version,
        version,
        default_l1_resource_bounds,
    )
    .unwrap();

    // Validation scenario: fallible validation.
    let block_context = BlockContext::create_for_account_testing();
    let base_gas = calculate_actual_gas(&tx_execution_info, &block_context, validate);
    assert!(
        base_gas > u64_from_usize(get_tx_resources(TransactionType::InvokeFunction).n_steps).into()
    );
    let (actual_gas_used, actual_fee) = gas_and_fee(base_gas, validate, &fee_type);

    // The reported fee should be the actual cost, regardless of whether or not fee is charged.
    check_gas_and_fee(
        &block_context,
        &tx_execution_info,
        &fee_type,
        actual_gas_used,
        actual_fee,
        actual_fee,
    );
}

/// Test simulate / validate / charge_fee flag combinations during execution.
#[rstest]
#[case(TransactionVersion::ONE, FeeType::Eth)]
#[case(TransactionVersion::THREE, FeeType::Strk)]
fn test_simulate_validate_charge_fee_mid_execution(
    #[values(true, false)] only_query: bool,
    #[values(true, false)] validate: bool,
    #[values(true, false)] charge_fee: bool,
    // TODO(Dori, 1/1/2024): Add Cairo1 case, after price abstraction is implemented.
    #[values(CairoVersion::Cairo0)] cairo_version: CairoVersion,
    #[case] version: TransactionVersion,
    #[case] fee_type: FeeType,
    default_l1_resource_bounds: ValidResourceBounds,
) {
    let block_context = BlockContext::create_for_account_testing();
    let chain_info = &block_context.chain_info;
    let gas_price = block_context.block_info.gas_prices.l1_gas_price(&fee_type);
    let FlavorTestInitialState {
        mut state,
        account_address,
        test_contract_address,
        mut nonce_manager,
        ..
    } = create_flavors_test_state(chain_info, cairo_version);

    // If charge_fee is false, test that balance indeed doesn't change.
    let (current_balance, _) = state
        .get_fee_token_balance(account_address, chain_info.fee_token_address(&fee_type))
        .unwrap();

    // Execution scenarios.
    // 1. Execution fails due to logic error.
    // 2. Execution fails due to out-of-resources error, due to max sender bounds, mid-run.
    // 3. Execution fails due to out-of-resources error, due to max block bounds, mid-run.
    let execution_base_args = invoke_tx_args! {
        max_fee: MAX_FEE,
        resource_bounds: default_l1_resource_bounds,
        sender_address: account_address,
        version,
    };

    // First scenario: logic error. Should result in revert; actual fee should be shown.
    let tx = executable_invoke_tx(invoke_tx_args! {
        calldata: recurse_calldata(test_contract_address, true, 3),
        nonce: nonce_manager.next(account_address),
        ..execution_base_args.clone()
    });
    let account_tx = AccountTransaction {
        tx,
        execution_flags: ExecutionFlags {
            only_query,
            charge_fee,
            validate,
            strict_nonce_check: true,
        },
    };
    let tx_execution_info = account_tx.execute(&mut state, &block_context).unwrap();
    let base_gas = calculate_actual_gas(&tx_execution_info, &block_context, validate);
    let (revert_gas_used, revert_fee) = gas_and_fee(base_gas, validate, &fee_type);
    assert!(
        base_gas > u64_from_usize(get_tx_resources(TransactionType::InvokeFunction).n_steps).into()
    );
    assert!(tx_execution_info.is_reverted());
    check_gas_and_fee(
        &block_context,
        &tx_execution_info,
        &fee_type,
        revert_gas_used,
        revert_fee,
        revert_fee,
    );
    let current_balance = check_balance(
        current_balance,
        &state,
        account_address,
        &block_context.chain_info,
        &fee_type,
        charge_fee,
    );

    // Second scenario: limit resources via sender bounds. Should revert if and only if step limit
    // is derived from sender bounds (`charge_fee` mode).
    let (gas_bound, fee_bound) = gas_and_fee(6543_u32.into(), validate, &fee_type);
    // If `charge_fee` is true, execution is limited by sender bounds, so less resources will be
    // used. Otherwise, execution is limited by block bounds, so more resources will be used.
    let (limited_gas_used, limited_fee) = gas_and_fee(8195_u32.into(), validate, &fee_type);
    let (unlimited_gas_used, unlimited_fee) = gas_and_fee(
        u64_from_usize(
            get_const_syscall_resources(SyscallSelector::CallContract).n_steps
                + get_tx_resources(TransactionType::InvokeFunction).n_steps
                + 5722,
        )
        .into(),
        validate,
        &fee_type,
    );
    let tx = executable_invoke_tx(invoke_tx_args! {
        max_fee: fee_bound,
        resource_bounds: l1_resource_bounds(gas_bound, gas_price.into()),
        calldata: recurse_calldata(test_contract_address, false, 1000),
        nonce: nonce_manager.next(account_address),
        ..execution_base_args.clone()
    });
    let account_tx = AccountTransaction {
        tx,
        execution_flags: ExecutionFlags {
            only_query,
            charge_fee,
            validate,
            strict_nonce_check: true,
        },
    };
    let tx_execution_info = account_tx.execute(&mut state, &block_context).unwrap();
    assert_eq!(tx_execution_info.is_reverted(), charge_fee);
    if charge_fee {
        assert!(
            tx_execution_info
                .revert_error
                .clone()
                .unwrap()
                .to_string()
                .contains("no remaining steps")
        );
    }
    check_gas_and_fee(
        &block_context,
        &tx_execution_info,
        &fee_type,
        // In case `charge_fee = false` we completely ignore the sender bounds when executing the
        // transaction. If `charge_fee` is true, we limit the transaction steps according to the
        // sender bounds. However, there are other resources that consumes gas (e.g. L1 data
        // availability), hence the actual resources may exceed the senders bounds after all.
        if charge_fee { limited_gas_used } else { unlimited_gas_used },
        if charge_fee { fee_bound } else { unlimited_fee },
        // Complete resources used are reported as receipt.resources; but only the
        // charged final fee is shown in actual_fee.
        if charge_fee { limited_fee } else { unlimited_fee },
    );
    let current_balance =
        check_balance(current_balance, &state, account_address, chain_info, &fee_type, charge_fee);

    // Third scenario: only limit is block bounds. Expect resources consumed to be identical,
    // whether or not `charge_fee` is true.
    let mut low_step_block_context = block_context.clone();
    low_step_block_context.versioned_constants.invoke_tx_max_n_steps = 10000;
    let (huge_gas_limit, huge_fee) = gas_and_fee(100000_u32.into(), validate, &fee_type);
    // Gas usage does not depend on `validate` flag in this scenario, because we reach the block
    // step limit during execution anyway. The actual limit when execution phase starts is slightly
    // lower when `validate` is true, but this is not reflected in the actual gas usage.
    let invoke_tx_max_n_steps_as_u64: u64 =
        low_step_block_context.versioned_constants.invoke_tx_max_n_steps.into();
    let block_limit_gas = (invoke_tx_max_n_steps_as_u64 + 1652).into();
    let block_limit_fee = get_fee_by_gas_vector(
        &block_context.block_info,
        GasVector::from_l1_gas(block_limit_gas),
        &fee_type,
        block_context.to_tx_context(&account_tx).effective_tip(),
    );

    let tx = executable_invoke_tx(invoke_tx_args! {
        max_fee: huge_fee,
        resource_bounds: l1_resource_bounds(huge_gas_limit, gas_price.into()),
        calldata: recurse_calldata(test_contract_address, false, 10000),
        nonce: nonce_manager.next(account_address),
        ..execution_base_args
    });
    let account_tx = AccountTransaction {
        tx,
        execution_flags: ExecutionFlags {
            only_query,
            charge_fee,
            validate,
            strict_nonce_check: true,
        },
    };
    let tx_execution_info = account_tx.execute(&mut state, &low_step_block_context).unwrap();
    assert!(
        tx_execution_info.revert_error.clone().unwrap().to_string().contains("no remaining steps")
    );
    // Complete resources used are reported as receipt.resources; but only the charged
    // final fee is shown in actual_fee. As a sanity check, verify that the fee derived directly
    // from the consumed resources is also equal to the expected fee.
    check_gas_and_fee(
        &block_context,
        &tx_execution_info,
        &fee_type,
        block_limit_gas,
        block_limit_fee,
        block_limit_fee,
    );
    check_balance(current_balance, &state, account_address, chain_info, &fee_type, charge_fee);
}

#[rstest]
#[case(TransactionVersion::ONE, FeeType::Eth, true)]
#[case(TransactionVersion::THREE, FeeType::Strk, false)]
fn test_simulate_validate_charge_fee_post_execution(
    #[values(true, false)] only_query: bool,
    #[values(true, false)] validate: bool,
    #[values(true, false)] charge_fee: bool,
    // TODO(Dori, 1/1/2024): Add Cairo1 case, after price abstraction is implemented.
    #[values(CairoVersion::Cairo0)] cairo_version: CairoVersion,
    #[case] version: TransactionVersion,
    #[case] fee_type: FeeType,
    #[case] is_deprecated: bool,
) {
    let mut block_context = BlockContext::create_for_account_testing();
    block_context.versioned_constants.allocation_cost = AllocationCost::ZERO;
    let gas_price = block_context.block_info.gas_prices.l1_gas_price(&fee_type);
    let chain_info = &block_context.chain_info;
    let fee_token_address = chain_info.fee_token_address(&fee_type);

    let FlavorTestInitialState {
        mut state,
        account_address,
        test_contract_address,
        mut nonce_manager,
        ..
    } = create_flavors_test_state(chain_info, cairo_version);

    // If charge_fee is false, test that balance indeed doesn't change.
    let (current_balance, _) =
        state.get_fee_token_balance(account_address, fee_token_address).unwrap();

    // Post-execution scenarios:
    // 1. Consumed too many resources (more than resource bounds).
    // 2. Balance is lower than actual fee.

    // First scenario: resource overdraft.
    // If `charge_fee` is false - we do not revert, and simply report the fee and resources as used.
    // If `charge_fee` is true, we revert, charge the maximal allowed fee (derived from sender
    // bounds), and report resources base on execution steps reverted + other overhead.
    let invoke_steps = u64_from_usize(get_tx_resources(TransactionType::InvokeFunction).n_steps);
    let base_gas_bound = (invoke_steps + 2479).into();
    let (just_not_enough_gas_bound, just_not_enough_fee_bound) =
        gas_and_fee(base_gas_bound, validate, &fee_type);
    // `__validate__` and overhead resources + number of reverted steps, comes out slightly more
    // than the gas bound.
    let (revert_gas_usage, revert_fee) =
        gas_and_fee((invoke_steps + 4122).into(), validate, &fee_type);
    let (unlimited_gas_used, unlimited_fee) = gas_and_fee(
        (invoke_steps
            + u64_from_usize(
                get_const_syscall_resources(SyscallSelector::CallContract).n_steps + 4122,
            ))
        .into(),
        validate,
        &fee_type,
    );
    let tx = executable_invoke_tx(invoke_tx_args! {
        max_fee: just_not_enough_fee_bound,
        resource_bounds: l1_resource_bounds(just_not_enough_gas_bound, gas_price.into()),
        calldata: recurse_calldata(test_contract_address, false, 600),
        nonce: nonce_manager.next(account_address),
        sender_address: account_address,
        version,
    });
    let account_tx = AccountTransaction {
        tx,
        execution_flags: ExecutionFlags {
            only_query,
            charge_fee,
            validate,
            strict_nonce_check: true,
        },
    };
    let tx_execution_info = account_tx.execute(&mut state, &block_context).unwrap();
    assert_eq!(tx_execution_info.is_reverted(), charge_fee);
    if charge_fee {
        let expected_error_prefix =
            &format!("Insufficient max {resource}", resource = Resource::L1Gas);
        assert!(tx_execution_info.revert_error.clone().unwrap().to_string().starts_with(
            if is_deprecated { "Insufficient max fee" } else { expected_error_prefix }
        ));
    }

    check_gas_and_fee(
        &block_context,
        &tx_execution_info,
        &fee_type,
        if charge_fee { revert_gas_usage } else { unlimited_gas_used },
        if charge_fee { just_not_enough_fee_bound } else { unlimited_fee },
        if charge_fee { revert_fee } else { unlimited_fee },
    );
    let current_balance =
        check_balance(current_balance, &state, account_address, chain_info, &fee_type, charge_fee);

    // Second scenario: balance too low.
    // Execute a transfer, and make sure we get the expected result.
    let (success_actual_gas, actual_fee) = gas_and_fee(
        (u64_from_usize(get_const_syscall_resources(SyscallSelector::CallContract).n_steps)
            + invoke_steps
            + 4332)
            .into(),
        validate,
        &fee_type,
    );
    let (fail_actual_gas, fail_actual_fee) =
        gas_and_fee((invoke_steps + 2239).into(), validate, &fee_type);
    assert!(felt!(actual_fee.0) < current_balance);
    let transfer_amount = current_balance - Felt::from(actual_fee.0 / 2);
    let recipient = felt!(7_u8);
    let transfer_calldata = create_calldata(
        fee_token_address,
        "transfer",
        &[
            recipient, // Calldata: to.
            transfer_amount,
            felt!(0_u8),
        ],
    );
    let tx = executable_invoke_tx(invoke_tx_args! {
        max_fee: actual_fee,
        resource_bounds: l1_resource_bounds(success_actual_gas, gas_price.into()),
        calldata: transfer_calldata,
        nonce: nonce_manager.next(account_address),
        sender_address: account_address,
        version,
    });
    let account_tx = AccountTransaction {
        tx,
        execution_flags: ExecutionFlags {
            only_query,
            charge_fee,
            validate,
            strict_nonce_check: true,
        },
    };
    let tx_execution_info = account_tx.execute(&mut state, &block_context).unwrap();
    assert_eq!(tx_execution_info.is_reverted(), charge_fee);

    if charge_fee {
        assert!(
            tx_execution_info
                .revert_error
                .clone()
                .unwrap()
                .to_string()
                .contains("Insufficient fee token balance.")
        );
    }
    check_gas_and_fee(
        &block_context,
        &tx_execution_info,
        &fee_type,
        // Since the failure was due to insufficient balance, the actual fee remains the same
        // regardless of whether or not the transaction was reverted.
        // The reported gas consumed, on the other hand, is much lower if the transaction was
        // reverted.
        if charge_fee { fail_actual_gas } else { success_actual_gas },
        actual_fee,
        if charge_fee { fail_actual_fee } else { actual_fee },
    );
    check_balance(
        current_balance,
        &state,
        account_address,
        chain_info,
        &fee_type,
        // Even if `charge_fee` is false, we expect balance to be reduced here; as in this case the
        // transaction will not be reverted, and the balance transfer should be applied.
        true,
    );
}
