use std::collections::HashMap;

use assert_matches::assert_matches;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use rstest::rstest;
use starknet_api::invoke_tx_args;
use starknet_api::transaction::{Fee, Resource, ValidResourceBounds};

use crate::blockifier::block::GasPrices;
use crate::context::BlockContext;
use crate::fee::fee_checks::{FeeCheckError, FeeCheckReportFields, PostExecutionReport};
use crate::fee::fee_utils::get_vm_resources_cost;
use crate::fee::receipt::TransactionReceipt;
use crate::fee::resources::{GasVector, GasVectorComputationMode};
use crate::test_utils::contracts::FeatureContract;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::{
    CairoVersion,
    BALANCE,
    DEFAULT_ETH_L1_DATA_GAS_PRICE,
    DEFAULT_ETH_L1_GAS_PRICE,
    DEFAULT_L1_DATA_GAS_MAX_AMOUNT,
    DEFAULT_L2_GAS_MAX_AMOUNT,
    DEFAULT_STRK_L1_GAS_PRICE,
    MAX_L1_GAS_AMOUNT,
};
use crate::transaction::test_utils::{account_invoke_tx, all_resource_bounds, l1_resource_bounds};
use crate::utils::u128_from_usize;
use crate::versioned_constants::VersionedConstants;

fn get_vm_resource_usage() -> ExecutionResources {
    ExecutionResources {
        n_steps: 10000,
        n_memory_holes: 0,
        builtin_instance_counter: HashMap::from([
            (BuiltinName::pedersen, 10),
            (BuiltinName::range_check, 24),
            (BuiltinName::ecdsa, 1),
            (BuiltinName::bitwise, 1),
            (BuiltinName::poseidon, 1),
        ]),
    }
}

#[test]
fn test_simple_get_vm_resource_usage() {
    let versioned_constants = VersionedConstants::create_for_account_testing();
    let mut vm_resource_usage = get_vm_resource_usage();
    let n_reverted_steps = 15;

    // Positive flow.
    // Verify calculation - in our case, n_steps is the heaviest resource.
    let l1_gas_by_vm_usage = (versioned_constants.vm_resource_fee_cost().n_steps
        * (u128_from_usize(vm_resource_usage.n_steps + n_reverted_steps)))
    .ceil()
    .to_integer();
    assert_eq!(
        GasVector::from_l1_gas(l1_gas_by_vm_usage),
        get_vm_resources_cost(
            &versioned_constants,
            &vm_resource_usage,
            n_reverted_steps,
            &GasVectorComputationMode::NoL2Gas
        )
        .unwrap()
    );

    // Another positive flow, this time the heaviest resource is range_check_builtin.
    let n_reverted_steps = 0;
    vm_resource_usage.n_steps =
        vm_resource_usage.builtin_instance_counter.get(&BuiltinName::range_check).unwrap() - 1;
    let l1_gas_by_vm_usage =
        vm_resource_usage.builtin_instance_counter.get(&BuiltinName::range_check).unwrap();
    assert_eq!(
        GasVector::from_l1_gas(u128_from_usize(*l1_gas_by_vm_usage)),
        get_vm_resources_cost(
            &versioned_constants,
            &vm_resource_usage,
            n_reverted_steps,
            &GasVectorComputationMode::NoL2Gas
        )
        .unwrap()
    );
}

#[test]
fn test_float_get_vm_resource_usage() {
    let versioned_constants = VersionedConstants::create_for_testing();
    let mut vm_resource_usage = get_vm_resource_usage();

    // Positive flow.
    // Verify calculation - in our case, n_steps is the heaviest resource.
    let n_reverted_steps = 300;
    let l1_gas_by_vm_usage = (versioned_constants.vm_resource_fee_cost().n_steps
        * u128_from_usize(vm_resource_usage.n_steps + n_reverted_steps))
    .ceil()
    .to_integer();
    assert_eq!(
        GasVector::from_l1_gas(l1_gas_by_vm_usage),
        get_vm_resources_cost(
            &versioned_constants,
            &vm_resource_usage,
            n_reverted_steps,
            &GasVectorComputationMode::NoL2Gas
        )
        .unwrap()
    );

    // Another positive flow, this time the heaviest resource is ecdsa_builtin.
    vm_resource_usage.n_steps = 200;
    let l1_gas_by_vm_usage =
        ((*versioned_constants.vm_resource_fee_cost().builtins.get(&BuiltinName::ecdsa).unwrap())
            * u128_from_usize(
                *vm_resource_usage.builtin_instance_counter.get(&BuiltinName::ecdsa).unwrap(),
            ))
        .ceil()
        .to_integer();

    assert_eq!(
        GasVector::from_l1_gas(l1_gas_by_vm_usage),
        get_vm_resources_cost(
            &versioned_constants,
            &vm_resource_usage,
            n_reverted_steps,
            &GasVectorComputationMode::NoL2Gas
        )
        .unwrap()
    );
}

/// Test the L1 gas limit bound, as applied to the case where both gas and data gas are consumed.
#[rstest]
#[case::no_dg_within_bounds(1000, 10, 10000, 0, 10000, false)]
#[case::no_dg_overdraft(1000, 10, 10001, 0, 10000, true)]
#[case::both_gases_within_bounds(1000, 10, 10000, 5000, 100000, false)]
#[case::both_gases_overdraft(1000, 10, 10000, 5000, 10000, true)]
#[case::expensive_dg_no_dg_within_bounds(10, 1000, 10, 0, 10, false)]
#[case::expensive_dg_with_dg_overdraft(10, 1000, 10, 1, 109, true)]
#[case::expensive_dg_with_dg_within_bounds(10, 1000, 10, 1, 110, false)]
fn test_discounted_gas_overdraft(
    #[case] gas_price: u128,
    #[case] data_gas_price: u128,
    #[case] l1_gas_used: usize,
    #[case] l1_data_gas_used: usize,
    #[case] gas_bound: u64,
    #[case] expect_failure: bool,
) {
    let mut block_context = BlockContext::create_for_account_testing();
    block_context.block_info.gas_prices = GasPrices::new(
        DEFAULT_ETH_L1_GAS_PRICE.try_into().unwrap(),
        gas_price.try_into().unwrap(),
        DEFAULT_ETH_L1_DATA_GAS_PRICE.try_into().unwrap(),
        data_gas_price.try_into().unwrap(),
        VersionedConstants::latest_constants()
            .convert_l1_to_l2_gas_price_round_up(DEFAULT_ETH_L1_GAS_PRICE)
            .try_into()
            .unwrap(),
        VersionedConstants::latest_constants()
            //TODO!(Aner): fix test parameters to allow using `gas_price` here!
            .convert_l1_to_l2_gas_price_round_up(DEFAULT_STRK_L1_GAS_PRICE)
            .try_into()
            .unwrap(),
    );

    let account = FeatureContract::AccountWithoutValidations(CairoVersion::Cairo0);
    let mut state = test_state(&block_context.chain_info, BALANCE, &[(account, 1)]);
    let tx = account_invoke_tx(invoke_tx_args! {
        sender_address: account.get_instance_address(0),
        resource_bounds: l1_resource_bounds(gas_bound, gas_price * 10),
    });

    let tx_receipt = TransactionReceipt {
        fee: Fee(7),
        gas: GasVector {
            l1_gas: u128_from_usize(l1_gas_used),
            l1_data_gas: u128_from_usize(l1_data_gas_used),
            ..Default::default()
        },
        ..Default::default()
    };
    let charge_fee = true;
    let report = PostExecutionReport::new(
        &mut state,
        &block_context.to_tx_context(&tx),
        &tx_receipt,
        charge_fee,
    )
    .unwrap();

    if expect_failure {
        let error = report.error().unwrap();
        let expected_actual_amount = u128_from_usize(l1_gas_used)
            + (u128_from_usize(l1_data_gas_used) * data_gas_price) / gas_price;
        assert_matches!(
            error, FeeCheckError::MaxGasAmountExceeded { resource, max_amount, actual_amount }
            if max_amount == u128::from(gas_bound) && actual_amount == expected_actual_amount && resource == Resource::L1Gas
        )
    } else {
        assert_matches!(report.error(), None);
    }
}

/// Test all resource gas limit bounds, This applies to the scenario where all resources are signed.
/// The transaction is signed on all resources, and the gas limit is set to default for each
/// resource. If the gas used exceeds the limit, the post-execution validation should fail.
#[rstest]
#[case::l1_bound_overdraft(2*MAX_L1_GAS_AMOUNT, DEFAULT_L2_GAS_MAX_AMOUNT, DEFAULT_L1_DATA_GAS_MAX_AMOUNT, Some(Resource::L1Gas))]
#[case::l2_bound_overdraft(MAX_L1_GAS_AMOUNT, 2* DEFAULT_L2_GAS_MAX_AMOUNT, DEFAULT_L1_DATA_GAS_MAX_AMOUNT, Some(Resource::L2Gas))]
#[case::l1_data_bound_overdraft(
    MAX_L1_GAS_AMOUNT,
    DEFAULT_L2_GAS_MAX_AMOUNT,
    2*DEFAULT_L1_DATA_GAS_MAX_AMOUNT,
    Some(Resource::L1DataGas)
)]
#[case::no_overdraft(MAX_L1_GAS_AMOUNT/2, DEFAULT_L2_GAS_MAX_AMOUNT/2, DEFAULT_L1_DATA_GAS_MAX_AMOUNT/2, None)]
fn test_post_execution_gas_overdraft_all_resource_bounds(
    all_resource_bounds: ValidResourceBounds,
    #[case] l1_gas_used: u64,
    #[case] l2_gas_used: u64,
    #[case] l1_data_gas_used: u64,
    #[case] resource_out_of_bounds: Option<Resource>,
) {
    let block_context = BlockContext::create_for_account_testing();

    let account = FeatureContract::AccountWithoutValidations(CairoVersion::Cairo0);
    let mut state = test_state(&block_context.chain_info, BALANCE, &[(account, 1)]);
    let tx = account_invoke_tx(invoke_tx_args! {
        sender_address: account.get_instance_address(0),
        resource_bounds: all_resource_bounds,
    });

    let tx_receipt = TransactionReceipt {
        fee: Fee(0),
        gas: GasVector {
            l1_gas: l1_gas_used.into(),
            l2_gas: l2_gas_used.into(),
            l1_data_gas: l1_data_gas_used.into(),
        },
        ..Default::default()
    };
    let charge_fee = true;
    let report = PostExecutionReport::new(
        &mut state,
        &block_context.to_tx_context(&tx),
        &tx_receipt,
        charge_fee,
    )
    .unwrap();

    match resource_out_of_bounds {
        Some(resource_value) => {
            let error = report.error().unwrap();
            assert_matches!(error, FeeCheckError::MaxGasAmountExceeded { resource, .. } if resource == resource_value);
        }
        None => {
            assert_matches!(report.error(), None);
        }
    }
}
