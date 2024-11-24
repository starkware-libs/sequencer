use assert_matches::assert_matches;
use cairo_vm::types::builtin_name::BuiltinName;
use rstest::rstest;
use starknet_api::block::{FeeType, GasPrice, NonzeroGasPrice};
use starknet_api::execution_resources::{GasAmount, GasVector};
use starknet_api::invoke_tx_args;
use starknet_api::transaction::fields::{
    AllResourceBounds,
    Fee,
    GasVectorComputationMode,
    Resource,
    ResourceBounds,
    ValidResourceBounds,
};

use crate::blockifier::block::validated_gas_prices;
use crate::context::BlockContext;
use crate::fee::fee_checks::{FeeCheckError, FeeCheckReportFields, PostExecutionReport};
use crate::fee::fee_utils::{get_fee_by_gas_vector, get_vm_resources_cost};
use crate::fee::receipt::TransactionReceipt;
use crate::test_utils::contracts::FeatureContract;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::{
    gas_vector_from_vm_usage,
    get_vm_resource_usage,
    CairoVersion,
    BALANCE,
    DEFAULT_ETH_L1_DATA_GAS_PRICE,
    DEFAULT_ETH_L1_GAS_PRICE,
    DEFAULT_L1_DATA_GAS_MAX_AMOUNT,
    DEFAULT_L1_GAS_AMOUNT,
    DEFAULT_L2_GAS_MAX_AMOUNT,
    DEFAULT_STRK_L1_GAS_PRICE,
};
use crate::transaction::test_utils::{
    account_invoke_tx,
    all_resource_bounds,
    block_context,
    l1_resource_bounds,
};
use crate::utils::u64_from_usize;
use crate::versioned_constants::VersionedConstants;

#[rstest]
fn test_simple_get_vm_resource_usage(
    #[values(GasVectorComputationMode::NoL2Gas, GasVectorComputationMode::All)]
    gas_vector_computation_mode: GasVectorComputationMode,
) {
    let versioned_constants = VersionedConstants::create_for_account_testing();
    let mut vm_resource_usage = get_vm_resource_usage();
    let n_reverted_steps = 15;

    // Positive flow.
    // Verify calculation - in our case, n_steps is the heaviest resource.
    let vm_usage_in_l1_gas = (versioned_constants.vm_resource_fee_cost().n_steps
        * (u64_from_usize(vm_resource_usage.n_steps + n_reverted_steps)))
    .ceil()
    .to_integer()
    .into();
    let expected_gas_vector = gas_vector_from_vm_usage(
        vm_usage_in_l1_gas,
        &gas_vector_computation_mode,
        &versioned_constants,
    );
    assert_eq!(
        expected_gas_vector,
        get_vm_resources_cost(
            &versioned_constants,
            &vm_resource_usage,
            n_reverted_steps,
            &gas_vector_computation_mode
        )
    );

    // Another positive flow, this time the heaviest resource is range_check_builtin.
    let n_reverted_steps = 0;
    vm_resource_usage.n_steps =
        vm_resource_usage.builtin_instance_counter.get(&BuiltinName::range_check).unwrap() - 1;
    let vm_usage_in_l1_gas = u64_from_usize(
        *vm_resource_usage.builtin_instance_counter.get(&BuiltinName::range_check).unwrap(),
    )
    .into();
    let expected_gas_vector = gas_vector_from_vm_usage(
        vm_usage_in_l1_gas,
        &gas_vector_computation_mode,
        &versioned_constants,
    );
    assert_eq!(
        expected_gas_vector,
        get_vm_resources_cost(
            &versioned_constants,
            &vm_resource_usage,
            n_reverted_steps,
            &gas_vector_computation_mode
        )
    );
}

#[rstest]
fn test_float_get_vm_resource_usage(
    #[values(GasVectorComputationMode::NoL2Gas, GasVectorComputationMode::All)]
    gas_vector_computation_mode: GasVectorComputationMode,
) {
    let versioned_constants = VersionedConstants::create_for_testing();
    let mut vm_resource_usage = get_vm_resource_usage();

    // Positive flow.
    // Verify calculation - in our case, n_steps is the heaviest resource.
    let n_reverted_steps = 300;
    let vm_usage_in_l1_gas = (versioned_constants.vm_resource_fee_cost().n_steps
        * u64_from_usize(vm_resource_usage.n_steps + n_reverted_steps))
    .ceil()
    .to_integer()
    .into();
    let expected_gas_vector = gas_vector_from_vm_usage(
        vm_usage_in_l1_gas,
        &gas_vector_computation_mode,
        &versioned_constants,
    );
    assert_eq!(
        expected_gas_vector,
        get_vm_resources_cost(
            &versioned_constants,
            &vm_resource_usage,
            n_reverted_steps,
            &gas_vector_computation_mode
        )
    );

    // Another positive flow, this time the heaviest resource is ecdsa_builtin.
    vm_resource_usage.n_steps = 200;
    let vm_usage_in_l1_gas =
        ((*versioned_constants.vm_resource_fee_cost().builtins.get(&BuiltinName::ecdsa).unwrap())
            * u64_from_usize(
                *vm_resource_usage.builtin_instance_counter.get(&BuiltinName::ecdsa).unwrap(),
            ))
        .ceil()
        .to_integer()
        .into();
    let expected_gas_vector = gas_vector_from_vm_usage(
        vm_usage_in_l1_gas,
        &gas_vector_computation_mode,
        &versioned_constants,
    );
    assert_eq!(
        expected_gas_vector,
        get_vm_resources_cost(
            &versioned_constants,
            &vm_resource_usage,
            n_reverted_steps,
            &gas_vector_computation_mode
        )
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
    #[case] l1_gas_used: u64,
    #[case] l1_data_gas_used: u64,
    #[case] gas_bound: u64,
    #[case] expect_failure: bool,
) {
    let (l1_gas_used, l1_data_gas_used, gas_bound) =
        (GasAmount(l1_gas_used), GasAmount(l1_data_gas_used), GasAmount(gas_bound));
    let (gas_price, data_gas_price) = (
        NonzeroGasPrice::try_from(gas_price).unwrap(),
        NonzeroGasPrice::try_from(data_gas_price).unwrap(),
    );
    let mut block_context = BlockContext::create_for_account_testing();
    block_context.block_info.gas_prices = validated_gas_prices(
        DEFAULT_ETH_L1_GAS_PRICE,
        gas_price,
        DEFAULT_ETH_L1_DATA_GAS_PRICE,
        data_gas_price,
        VersionedConstants::latest_constants()
            .convert_l1_to_l2_gas_price_round_up(DEFAULT_ETH_L1_GAS_PRICE.into())
            .try_into()
            .unwrap(),
        VersionedConstants::latest_constants()
            //TODO!(Aner): fix test parameters to allow using `gas_price` here!
            .convert_l1_to_l2_gas_price_round_up(DEFAULT_STRK_L1_GAS_PRICE.into())
            .try_into()
            .unwrap(),
    );

    let account = FeatureContract::AccountWithoutValidations(CairoVersion::Cairo0);
    let mut state = test_state(&block_context.chain_info, BALANCE, &[(account, 1)]);
    let tx = account_invoke_tx(invoke_tx_args! {
        sender_address: account.get_instance_address(0),
        resource_bounds: l1_resource_bounds(gas_bound, (gas_price.get().0 * 10).into()),
    });

    let tx_receipt = TransactionReceipt {
        fee: Fee(7),
        gas: GasVector { l1_gas: l1_gas_used, l1_data_gas: l1_data_gas_used, ..Default::default() },
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
        let expected_actual_amount = l1_gas_used
            + (l1_data_gas_used.checked_mul(data_gas_price.into()).unwrap())
                .checked_div(gas_price)
                .unwrap();
        assert_matches!(
            error, FeeCheckError::MaxGasAmountExceeded { resource, max_amount, actual_amount }
            if max_amount == gas_bound
             && actual_amount == expected_actual_amount
             && resource == Resource::L1Gas
        )
    } else {
        assert_matches!(report.error(), None);
    }
}

/// Test all resource gas limit bounds, This applies to the scenario where all resources are signed.
/// The transaction is signed on all resources, and the gas limit is set to default for each
/// resource. If the gas used exceeds the limit, the post-execution validation should fail.
#[rstest]
#[case::l1_bound_overdraft(
    (2 * DEFAULT_L1_GAS_AMOUNT.0).into(),
    DEFAULT_L2_GAS_MAX_AMOUNT,
    DEFAULT_L1_DATA_GAS_MAX_AMOUNT,
    Some(Resource::L1Gas)
)]
#[case::l2_bound_overdraft(
    DEFAULT_L1_GAS_AMOUNT,
    (2 * DEFAULT_L2_GAS_MAX_AMOUNT.0).into(),
    DEFAULT_L1_DATA_GAS_MAX_AMOUNT,
    Some(Resource::L2Gas)
)]
#[case::l1_data_bound_overdraft(
    DEFAULT_L1_GAS_AMOUNT,
    DEFAULT_L2_GAS_MAX_AMOUNT,
    (2 * DEFAULT_L1_DATA_GAS_MAX_AMOUNT.0).into(),
    Some(Resource::L1DataGas)
)]
#[case::no_overdraft(
    (DEFAULT_L1_GAS_AMOUNT.0 / 2).into(),
    (DEFAULT_L2_GAS_MAX_AMOUNT.0 / 2).into(),
    (DEFAULT_L1_DATA_GAS_MAX_AMOUNT.0 / 2).into(),
    None
)]
fn test_post_execution_gas_overdraft_all_resource_bounds(
    all_resource_bounds: ValidResourceBounds,
    #[case] l1_gas_used: GasAmount,
    #[case] l2_gas_used: GasAmount,
    #[case] l1_data_gas_used: GasAmount,
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
        gas: GasVector { l1_gas: l1_gas_used, l2_gas: l2_gas_used, l1_data_gas: l1_data_gas_used },
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

#[rstest]
#[case::happy_flow_l1_gas_only(10, 0, 0, 10, 2*10)]
#[case::happy_flow_no_l2_gas(10, 20, 0, 10 + 3*20, 2*10 + 4*20)]
#[case::happy_flow_all(10, 20, 30, 10 + 3*20 + 5*30, 2*10 + 4*20 + 6*30)]
fn test_get_fee_by_gas_vector_regression(
    #[case] l1_gas: u64,
    #[case] l1_data_gas: u64,
    #[case] l2_gas: u64,
    #[case] expected_fee_eth: u128,
    #[case] expected_fee_strk: u128,
) {
    let mut block_info = BlockContext::create_for_account_testing().block_info;
    block_info.gas_prices = validated_gas_prices(
        1_u8.try_into().unwrap(),
        2_u8.try_into().unwrap(),
        3_u8.try_into().unwrap(),
        4_u8.try_into().unwrap(),
        5_u8.try_into().unwrap(),
        6_u8.try_into().unwrap(),
    );
    let gas_vector =
        GasVector { l1_gas: l1_gas.into(), l1_data_gas: l1_data_gas.into(), l2_gas: l2_gas.into() };
    assert_eq!(
        get_fee_by_gas_vector(&block_info, gas_vector, &FeeType::Eth),
        Fee(expected_fee_eth)
    );
    assert_eq!(
        get_fee_by_gas_vector(&block_info, gas_vector, &FeeType::Strk),
        Fee(expected_fee_strk)
    );
}

#[rstest]
#[should_panic(expected = "L1Gas cost overflowed")]
#[case::l1_overflows(u64::MAX, 0, 0)]
#[should_panic(expected = "L1DataGas cost overflowed")]
#[case::l1_data_overflows(0, u64::MAX, 0)]
#[should_panic(expected = "L2Gas cost overflowed")]
#[case::l2_gas_overflows(0, 0, u64::MAX)]
fn test_get_fee_by_gas_vector_overflow(
    #[case] l1_gas: u64,
    #[case] l1_data_gas: u64,
    #[case] l2_gas: u64,
) {
    let huge_gas_price = NonzeroGasPrice::try_from(2_u128 * u128::from(u64::MAX)).unwrap();
    let mut block_info = BlockContext::create_for_account_testing().block_info;
    block_info.gas_prices = validated_gas_prices(
        huge_gas_price,
        huge_gas_price,
        huge_gas_price,
        huge_gas_price,
        huge_gas_price,
        huge_gas_price,
    );
    let gas_vector =
        GasVector { l1_gas: l1_gas.into(), l1_data_gas: l1_data_gas.into(), l2_gas: l2_gas.into() };
    assert_eq!(get_fee_by_gas_vector(&block_info, gas_vector, &FeeType::Eth), Fee(u128::MAX));
}

#[rstest]
#[case::default(
    VersionedConstants::create_for_account_testing().default_initial_gas_cost(),
    GasVectorComputationMode::NoL2Gas
)]
#[case::from_l2_gas(4321, GasVectorComputationMode::All)]
fn test_initial_sierra_gas(
    #[case] expected: u64,
    #[case] gas_mode: GasVectorComputationMode,
    block_context: BlockContext,
) {
    let resource_bounds = match gas_mode {
        GasVectorComputationMode::NoL2Gas => ValidResourceBounds::L1Gas(ResourceBounds {
            max_amount: GasAmount(1234),
            max_price_per_unit: GasPrice(56),
        }),
        GasVectorComputationMode::All => ValidResourceBounds::AllResources(AllResourceBounds {
            l2_gas: ResourceBounds {
                max_amount: GasAmount(expected),
                max_price_per_unit: GasPrice(1),
            },
            ..Default::default()
        }),
    };
    let account_tx = account_invoke_tx(invoke_tx_args!(resource_bounds));
    let actual = block_context.to_tx_context(&account_tx).initial_sierra_gas();
    assert_eq!(actual, expected)
}
