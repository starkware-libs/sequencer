use std::collections::{HashMap, HashSet};

use assert_matches::assert_matches;
use rstest::{fixture, rstest};
use starknet_api::execution_resources::GasAmount;
use starknet_api::transaction::fields::Fee;
use starknet_api::{class_hash, contract_address, storage_key};

use super::BouncerConfig;
use crate::blockifier::transaction_executor::TransactionExecutorError;
use crate::bouncer::{
    verify_tx_weights_within_max_capacity,
    Bouncer,
    BouncerWeights,
    CasmHashComputationData,
    TxWeights,
};
use crate::context::BlockContext;
use crate::execution::call_info::ExecutionSummary;
use crate::fee::resources::{ComputationResources, TransactionResources};
use crate::state::cached_state::{CachedState, StateChangesKeys, TransactionalState};
use crate::test_utils::dict_state_reader::DictStateReader;
use crate::test_utils::initial_test_state::test_state;
use crate::transaction::errors::TransactionExecutionError;

#[fixture]
fn block_context() -> BlockContext {
    BlockContext::create_for_account_testing()
}

#[fixture]
fn state(block_context: BlockContext) -> CachedState<DictStateReader> {
    test_state(&block_context.chain_info, Fee(0), &[])
}

#[fixture]
fn block_max_capacity() -> BouncerWeights {
    BouncerWeights {
        l1_gas: 20,
        message_segment_length: 20,
        n_events: 20,
        state_diff_size: 20,
        sierra_gas: GasAmount(20),
        n_txs: 20,
    }
}

#[fixture]
fn bouncer_config(block_max_capacity: BouncerWeights) -> BouncerConfig {
    BouncerConfig { block_max_capacity }
}

#[rstest]
fn test_block_weights_has_room_sierra_gas(block_max_capacity: BouncerWeights) {
    let bouncer_weights = BouncerWeights {
        l1_gas: 7,
        message_segment_length: 10,
        n_events: 2,
        state_diff_size: 7,
        sierra_gas: GasAmount(7),
        n_txs: 7,
    };

    assert!(block_max_capacity.has_room(bouncer_weights));

    let bouncer_weights_exceeds_max = BouncerWeights {
        l1_gas: 5,
        message_segment_length: 5,
        n_events: 5,
        state_diff_size: 5,
        sierra_gas: GasAmount(25),
        n_txs: 5,
    };

    assert!(!block_max_capacity.has_room(bouncer_weights_exceeds_max));
}

#[rstest]
#[case::has_room(19, true)]
#[case::at_max(20, true)]
#[case::no_room(21, false)]
fn test_block_weights_has_room_n_txs(
    #[case] n_txs: usize,
    #[case] has_room: bool,
    block_max_capacity: BouncerWeights,
) {
    let bouncer_weights = BouncerWeights {
        l1_gas: 7,
        message_segment_length: 7,
        n_events: 7,
        state_diff_size: 7,
        sierra_gas: GasAmount(7),
        n_txs,
    };

    assert_eq!(block_max_capacity.has_room(bouncer_weights), has_room);
}

#[rstest]
#[case::empty_initial_bouncer(Bouncer::new(BouncerConfig::empty()))]
#[case::non_empty_initial_bouncer(Bouncer {
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
        n_txs: 1,
    },
    casm_hash_computation_data_sierra_gas: CasmHashComputationData{
        class_hash_to_casm_hash_computation_gas: HashMap::from([
        (class_hash!(0_u128), GasAmount(5))]),
        sierra_gas_without_casm_hash_computation: GasAmount(5),
    }
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
        n_txs: 1,
    };

    let class_hash_to_casm_hash_computation_gas_to_update =
        HashMap::from([(class_hash!(1_u128), GasAmount(1)), (class_hash!(2_u128), GasAmount(2))]);

    let casm_hash_computation_data_sierra_gas = CasmHashComputationData {
        class_hash_to_casm_hash_computation_gas: class_hash_to_casm_hash_computation_gas_to_update,
        sierra_gas_without_casm_hash_computation: GasAmount(6),
    };

    let tx_weights = TxWeights {
        bouncer_weights: weights_to_update,
        casm_hash_computation_data_sierra_gas: casm_hash_computation_data_sierra_gas.clone(),
    };

    let state_changes_keys_to_update =
        StateChangesKeys::create_for_testing(HashSet::from([contract_address!(1_u128)]));

    let mut updated_bouncer = initial_bouncer.clone();
    updated_bouncer.update(tx_weights, &execution_summary_to_update, &state_changes_keys_to_update);

    let mut expected_bouncer = initial_bouncer;
    expected_bouncer
        .visited_storage_entries
        .extend(&execution_summary_to_update.visited_storage_entries);
    expected_bouncer.state_changes_keys.extend(&state_changes_keys_to_update);
    expected_bouncer.accumulated_weights += weights_to_update;
    expected_bouncer
        .casm_hash_computation_data_sierra_gas
        .extend(casm_hash_computation_data_sierra_gas.clone());

    assert_eq!(updated_bouncer, expected_bouncer);
}

#[rstest]
#[case::positive_flow(GasAmount(1), "ok")]
#[case::block_full(GasAmount(11), "block_full")]
#[case::transaction_too_large(GasAmount(21), "too_large")]
fn test_bouncer_try_update_sierra_gas(
    #[case] added_gas: GasAmount,
    #[case] scenario: &'static str,
    block_context: BlockContext,
    block_max_capacity: BouncerWeights,
    bouncer_config: BouncerConfig,
    mut state: CachedState<DictStateReader>,
) {
    let accumulated_weights = BouncerWeights {
        l1_gas: 10,
        message_segment_length: 10,
        n_events: 10,
        state_diff_size: 10,
        sierra_gas: GasAmount(10),
        n_txs: 10,
    };

    let mut bouncer = Bouncer { accumulated_weights, bouncer_config, ..Bouncer::empty() };

    // Prepare the resources to be added to the bouncer.
    let execution_summary = ExecutionSummary::default();
    let tx_resources = TransactionResources {
        computation: ComputationResources { sierra_gas: added_gas, ..Default::default() },
        ..Default::default()
    };
    let mut transactional_state = TransactionalState::create_transactional(&mut state);
    let tx_state_changes_keys = transactional_state.to_state_diff().unwrap().state_maps.keys();

    // TODO(Yoni, 1/10/2024): simplify this test and move tx-too-large cases out.

    // Check that the transaction is not too large.
    let mut result = verify_tx_weights_within_max_capacity(
        &transactional_state,
        &execution_summary,
        &tx_resources,
        &tx_state_changes_keys,
        &bouncer.bouncer_config,
        &block_context.versioned_constants,
    )
    .map_err(TransactionExecutorError::TransactionExecutionError);
    let expected_weights =
        BouncerWeights { sierra_gas: added_gas, n_txs: 1, ..BouncerWeights::empty() };

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

    match scenario {
        "ok" => assert_matches!(result, Ok(())),
        "block_full" => assert_matches!(result, Err(TransactionExecutorError::BlockFull)),
        "too_large" => assert_matches!(result, Err(
                TransactionExecutorError::TransactionExecutionError(
                    TransactionExecutionError::TransactionTooLarge { max_capacity, tx_size }
                )
            ) if *max_capacity == block_max_capacity && *tx_size == expected_weights),
        _ => panic!("Unexpected scenario: {}", scenario),
    }
}

#[rstest]
fn test_bouncer_try_update_n_txs(
    block_context: BlockContext,
    bouncer_config: BouncerConfig,
    mut state: CachedState<DictStateReader>,
) {
    let accumulated_weights = BouncerWeights {
        l1_gas: 10,
        message_segment_length: 10,
        n_events: 10,
        state_diff_size: 10,
        sierra_gas: GasAmount(10),
        n_txs: 19,
    };

    let mut bouncer = Bouncer { accumulated_weights, bouncer_config, ..Bouncer::empty() };

    // Prepare first tx resources.
    let mut first_transactional_state = TransactionalState::create_transactional(&mut state);
    let first_tx_state_changes_keys =
        first_transactional_state.to_state_diff().unwrap().state_maps.keys();

    // Try to update the bouncer.
    let mut result = bouncer.try_update(
        &first_transactional_state,
        &first_tx_state_changes_keys,
        &ExecutionSummary::default(),
        &TransactionResources::default(),
        &block_context.versioned_constants,
    );
    assert_matches!(result, Ok(()));

    // Prepare second tx resources.
    let mut second_transactional_state =
        TransactionalState::create_transactional(&mut first_transactional_state);
    let second_tx_state_changes_keys =
        second_transactional_state.to_state_diff().unwrap().state_maps.keys();

    result = bouncer.try_update(
        &second_transactional_state,
        &second_tx_state_changes_keys,
        &ExecutionSummary::default(),
        &TransactionResources::default(),
        &block_context.versioned_constants,
    );

    assert_matches!(result, Err(TransactionExecutorError::BlockFull));
}
