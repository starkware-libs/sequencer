use std::collections::HashSet;

use assert_matches::assert_matches;
use rstest::rstest;
use starknet_api::execution_resources::GasAmount;
use starknet_api::transaction::fields::Fee;
use starknet_api::{class_hash, contract_address, storage_key};

use super::BouncerConfig;
use crate::blockifier::transaction_executor::{
    TransactionExecutorError,
    TransactionExecutorResult,
};
use crate::bouncer::{
    verify_tx_weights_within_max_capacity,
    Bouncer,
    BouncerWeights,
    BuiltinWeights,
};
use crate::context::BlockContext;
use crate::execution::call_info::ExecutionSummary;
use crate::fee::resources::{ComputationResources, TransactionResources};
use crate::state::cached_state::{CachedState, StateChangesKeys, TransactionalState};
use crate::test_utils::dict_state_reader::DictStateReader;
use crate::test_utils::initial_test_state::test_state;
use crate::transaction::errors::TransactionExecutionError;

#[test]
fn test_block_weights_has_room() {
    let max_bouncer_weights = BouncerWeights {
        l1_gas: 10,
        message_segment_length: 10,
        n_events: 10,
        state_diff_size: 10,
        sierra_gas: GasAmount(10),
        proving_gas: GasAmount(10),
    };

    let bouncer_weights = BouncerWeights {
        l1_gas: 7,
        message_segment_length: 10,
        n_events: 2,
        state_diff_size: 7,
        sierra_gas: GasAmount(7),
        proving_gas: GasAmount(5),
    };

    assert!(max_bouncer_weights.has_room(bouncer_weights));

    let bouncer_weights_exceeds_max = BouncerWeights {
        l1_gas: 5,
        message_segment_length: 5,
        n_events: 5,
        state_diff_size: 5,
        sierra_gas: GasAmount(15),
        proving_gas: GasAmount(5),
    };

    // Only the `sierra_gas` field exceeds the max here;
    // this test ensures that `has_room` correctly rejects it based on that field alone.
    assert!(!max_bouncer_weights.has_room(bouncer_weights_exceeds_max));
}

#[rstest]
#[case::empty_initial_bouncer(Bouncer::new(BouncerConfig::empty()))]
#[case::non_empty_initial_bouncer(Bouncer {
    executed_class_hashes: HashSet::from([class_hash!(0_u128)]),
    visited_storage_entries: HashSet::from([(
        contract_address!(0_u128),
        storage_key!(0_u128),
    )]),
    state_changes_keys: StateChangesKeys::create_for_testing(HashSet::from([
        contract_address!(0_u128),
    ])),
    bouncer_config: BouncerConfig::empty(),
    accumulated_weights: BouncerWeights {
        l1_gas: 10,
        message_segment_length: 10,
        n_events: 10,
        state_diff_size: 10,
        sierra_gas: GasAmount(10),
        proving_gas: GasAmount(10),
    },
})]
fn test_bouncer_update(#[case] initial_bouncer: Bouncer) {
    let execution_summary_to_update = ExecutionSummary {
        executed_class_hashes: HashSet::from([class_hash!(1_u128), class_hash!(2_u128)]),
        visited_storage_entries: HashSet::from([
            (contract_address!(1_u128), storage_key!(1_u128)),
            (contract_address!(2_u128), storage_key!(2_u128)),
        ]),
        ..Default::default()
    };

    let weights_to_update = BouncerWeights {
        l1_gas: 9,
        message_segment_length: 10,
        n_events: 1,
        state_diff_size: 2,
        sierra_gas: GasAmount(9),
        proving_gas: GasAmount(5),
    };

    let state_changes_keys_to_update =
        StateChangesKeys::create_for_testing(HashSet::from([contract_address!(1_u128)]));

    let mut updated_bouncer = initial_bouncer.clone();
    updated_bouncer.update(
        weights_to_update,
        &execution_summary_to_update,
        &state_changes_keys_to_update,
    );

    let mut expected_bouncer = initial_bouncer;
    expected_bouncer
        .executed_class_hashes
        .extend(&execution_summary_to_update.executed_class_hashes);
    expected_bouncer
        .visited_storage_entries
        .extend(&execution_summary_to_update.visited_storage_entries);
    expected_bouncer.state_changes_keys.extend(&state_changes_keys_to_update);
    expected_bouncer.accumulated_weights += weights_to_update;

    assert_eq!(updated_bouncer, expected_bouncer);
}

#[rstest]
#[case::positive_flow(GasAmount(1), "ok")]
#[case::block_full(GasAmount(11), "block_full")]
#[case::transaction_too_large(GasAmount(21), "too_large")]
fn test_bouncer_try_update_sierra_gas_based(
    #[case] added_gas: GasAmount,
    #[case] scenario: &'static str,
) {
    // Setup the bouncer.
    let (block_context, mut bouncer, config, mut state) = test_bouncer_try_update_setup_env();
    let mut transactional_state = TransactionalState::create_transactional(&mut state);

    // Prepare the resources to be added to the bouncer.
    let execution_summary = ExecutionSummary::default();

    let tx_resources = TransactionResources {
        computation: ComputationResources { sierra_gas: added_gas, ..Default::default() },
        ..Default::default()
    };

    let tx_state_changes_keys =
        transactional_state.get_actual_state_changes().unwrap().state_maps.keys();

    // TODO(Yoni, 1/10/2024): simplify this test and move tx-too-large cases out.

    // Check that the transaction is not too large.
    let mut result = verify_tx_weights_within_max_capacity(
        &transactional_state,
        &execution_summary,
        &tx_resources,
        &tx_state_changes_keys,
        &config,
        &block_context.versioned_constants,
    )
    .map_err(TransactionExecutorError::TransactionExecutionError);

    let expected_weights =
        BouncerWeights { sierra_gas: added_gas, proving_gas: added_gas, ..BouncerWeights::empty() };

    if result.is_ok() {
        // Try to update the bouncer.
        result = bouncer.try_update(
            &transactional_state,
            &tx_state_changes_keys,
            &execution_summary,
            &tx_resources,
            &block_context.versioned_constants,
        );
    }

    test_bouncer_try_update_assert_result(
        result,
        scenario,
        config.block_max_capacity,
        expected_weights,
    );
}

fn test_bouncer_try_update_setup_env()
-> (BlockContext, Bouncer, BouncerConfig, CachedState<DictStateReader>) {
    let block_context = BlockContext::create_for_account_testing();
    let state = test_state(&block_context.chain_info, Fee(0), &[]);

    let block_max_capacity = BouncerWeights {
        l1_gas: 20,
        message_segment_length: 20,
        n_events: 20,
        state_diff_size: 20,
        sierra_gas: GasAmount(20), // Arbitrary but static.
        proving_gas: GasAmount(140),
    };
    let bouncer_config =
        BouncerConfig { block_max_capacity, builtin_weights: BuiltinWeights::default() };

    let accumulated_weights = BouncerWeights {
        l1_gas: 10,
        message_segment_length: 10,
        n_events: 10,
        state_diff_size: 10,
        sierra_gas: GasAmount(10),
        proving_gas: GasAmount(10),
    };

    let bouncer =
        Bouncer { accumulated_weights, bouncer_config: bouncer_config.clone(), ..Bouncer::empty() };

    (block_context, bouncer, bouncer_config, state)
}

fn test_bouncer_try_update_assert_result(
    result: TransactionExecutorResult<()>,
    scenario: &str,
    max_capacity: BouncerWeights,
    tx_size: BouncerWeights,
) {
    match scenario {
        "ok" => assert_matches!(result, Ok(())),
        "block_full" => assert_matches!(result, Err(TransactionExecutorError::BlockFull)),
        "too_large" => assert_matches!(result, Err(
            TransactionExecutorError::TransactionExecutionError(
                TransactionExecutionError::TransactionTooLarge { max_capacity: mc, tx_size: ts }
            )
        )  if *mc == max_capacity && *ts == tx_size),
        _ => panic!("Unexpected scenario: {}", scenario),
    }
}
