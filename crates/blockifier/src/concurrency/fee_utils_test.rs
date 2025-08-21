use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_trivial_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use num_bigint::BigUint;
use rstest::rstest;
use starknet_api::block::FeeType;
use starknet_api::contract_class::compiled_class_hash::HashVersion;
use starknet_api::transaction::fields::{Fee, ValidResourceBounds};
use starknet_api::{felt, invoke_tx_args};
use starknet_types_core::felt::Felt;

use crate::concurrency::fee_utils::{add_fee_to_sequencer_balance, fill_sequencer_balance_reads};
use crate::concurrency::test_utils::create_fee_transfer_call_info;
use crate::context::BlockContext;
use crate::fee::fee_utils::get_sequencer_balance_keys;
use crate::state::cached_state::StateMaps;
use crate::state::state_api::StateReader;
use crate::test_utils::initial_test_state::{fund_account, test_state, test_state_inner};
use crate::test_utils::BALANCE;
use crate::transaction::test_utils::{
    block_context,
    default_all_resource_bounds,
    invoke_tx_with_default_flags,
};

#[rstest]
pub fn test_fill_sequencer_balance_reads(
    block_context: BlockContext,
    default_all_resource_bounds: ValidResourceBounds,
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1(RunnableCairo1::Casm))]
    erc20_version: CairoVersion,
) {
    let account =
        FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let account_tx = invoke_tx_with_default_flags(invoke_tx_args! {
        sender_address: account.get_instance_address(0),
        calldata: create_trivial_calldata(account.get_instance_address(0)),
        resource_bounds: default_all_resource_bounds,
    });
    let chain_info = &block_context.chain_info;
    let state = &mut test_state_inner(
        chain_info,
        BALANCE,
        &[(account.into(), 1)],
        &HashVersion::V2,
        erc20_version,
    );

    let sequencer_balance = Fee(100);
    let sequencer_address = block_context.block_info.sequencer_address;
    fund_account(chain_info, sequencer_address, sequencer_balance, &mut state.state);

    let mut concurrency_call_info = create_fee_transfer_call_info(state, &account_tx, true);
    let call_info = create_fee_transfer_call_info(state, &account_tx, false);

    assert_ne!(concurrency_call_info, call_info);

    fill_sequencer_balance_reads(
        &mut concurrency_call_info,
        (Felt::from(sequencer_balance), Felt::ZERO),
    );

    assert_eq!(concurrency_call_info, call_info);
}

#[rstest]
#[case::no_overflow(Fee(50_u128), felt!(100_u128), Felt::ZERO)]
#[case::overflow(Fee(150_u128), felt!(u128::MAX), felt!(5_u128))]
#[case::overflow_edge_case(Fee(500_u128), felt!(u128::MAX), felt!(u128::MAX-1))]
pub fn test_add_fee_to_sequencer_balance(
    #[case] actual_fee: Fee,
    #[case] sequencer_balance_low: Felt,
    #[case] sequencer_balance_high: Felt,
) {
    let block_context = BlockContext::create_for_account_testing();
    let account = FeatureContract::Empty(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let mut state = test_state(&block_context.chain_info, Fee(0), &[(account, 1)]);
    let (sequencer_balance_key_low, sequencer_balance_key_high) =
        get_sequencer_balance_keys(&block_context);

    let fee_token_address = block_context.chain_info.fee_token_address(&FeeType::Strk);
    let state_diff = &mut StateMaps::default();

    add_fee_to_sequencer_balance(
        fee_token_address,
        &mut state,
        actual_fee,
        &block_context,
        (sequencer_balance_low, sequencer_balance_high),
        account.get_instance_address(0),
        state_diff,
    );

    let new_sequencer_balance_value_low =
        state.get_storage_at(fee_token_address, sequencer_balance_key_low).unwrap();
    let new_sequencer_balance_value_high =
        state.get_storage_at(fee_token_address, sequencer_balance_key_high).unwrap();
    let expected_balance = (sequencer_balance_low + Felt::from(actual_fee.0)).to_biguint();

    let mask_128_bit = (BigUint::from(1_u8) << 128) - 1_u8;
    let expected_sequencer_balance_value_low = Felt::from(&expected_balance & mask_128_bit);
    let expected_sequencer_balance_value_high =
        sequencer_balance_high + Felt::from(&expected_balance >> 128);

    assert_eq!(new_sequencer_balance_value_low, expected_sequencer_balance_value_low);
    assert_eq!(new_sequencer_balance_value_high, expected_sequencer_balance_value_high);

    let expected_state_diff = StateMaps {
        storage: {
            let mut storage_entries = Vec::new();
            if new_sequencer_balance_value_low != sequencer_balance_low {
                storage_entries.push((
                    (fee_token_address, sequencer_balance_key_low),
                    new_sequencer_balance_value_low,
                ));
            }
            if new_sequencer_balance_value_high != sequencer_balance_high {
                storage_entries.push((
                    (fee_token_address, sequencer_balance_key_high),
                    new_sequencer_balance_value_high,
                ));
            }
            storage_entries
        }
        .into_iter()
        .collect(),
        ..StateMaps::default()
    };

    assert_eq!(state_diff, &expected_state_diff);
}
