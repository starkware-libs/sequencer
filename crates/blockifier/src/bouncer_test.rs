use std::collections::{HashMap, HashSet};

use assert_matches::assert_matches;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use cairo_vm::types::builtin_name::BuiltinName;
use rstest::{fixture, rstest};
use starknet_api::execution_resources::GasAmount;
use starknet_api::transaction::fields::Fee;
use starknet_api::{class_hash, contract_address, storage_key};

use super::BouncerConfig;
use crate::blockifier::transaction_executor::TransactionExecutorError;
use crate::bouncer::{
    get_tx_weights,
    verify_tx_weights_within_max_capacity,
    Bouncer,
    BouncerWeights,
    BuiltinWeights,
    CasmHashComputationData,
    TxWeights,
};
use crate::context::BlockContext;
use crate::execution::call_info::{BuiltinCounterMap, ExecutionSummary};
use crate::fee::resources::{ComputationResources, TransactionResources};
use crate::state::cached_state::{CachedState, StateChangesKeys, StateMaps, TransactionalState};
use crate::test_utils::contracts::FeatureContractData;
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
        proving_gas: GasAmount(20),
    }
}

#[fixture]
fn bouncer_config(block_max_capacity: BouncerWeights) -> BouncerConfig {
    BouncerConfig { block_max_capacity, builtin_weights: BuiltinWeights::default() }
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
        proving_gas: GasAmount(5),
    };

    assert!(block_max_capacity.has_room(bouncer_weights));

    let bouncer_weights_exceeds_max = BouncerWeights {
        l1_gas: 5,
        message_segment_length: 5,
        n_events: 5,
        state_diff_size: 5,
        n_txs: 5,
        sierra_gas: GasAmount(25),
        proving_gas: GasAmount(5),
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
        proving_gas: GasAmount(7),
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
        proving_gas: GasAmount(10),
    },
    casm_hash_computation_data_sierra_gas: CasmHashComputationData{
        class_hash_to_casm_hash_computation_gas: HashMap::from([
        (class_hash!(0_u128), GasAmount(5))]),
        gas_without_casm_hash_computation: GasAmount(5),
    },
    casm_hash_computation_data_proving_gas: CasmHashComputationData::empty(),
})]
fn test_bouncer_update(#[case] initial_bouncer: Bouncer) {
    // TODO(Aviv): Use expect! to avoid magic numbers.
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
        proving_gas: GasAmount(5),
    };

    let class_hash_to_casm_hash_computation_gas_to_update =
        HashMap::from([(class_hash!(1_u128), GasAmount(1)), (class_hash!(2_u128), GasAmount(2))]);

    let casm_hash_computation_data_sierra_gas = CasmHashComputationData {
        class_hash_to_casm_hash_computation_gas: class_hash_to_casm_hash_computation_gas_to_update,
        gas_without_casm_hash_computation: GasAmount(6),
    };
    let casm_hash_computation_data_proving_gas = CasmHashComputationData::empty();

    let tx_weights = TxWeights {
        bouncer_weights: weights_to_update,
        casm_hash_computation_data_sierra_gas: casm_hash_computation_data_sierra_gas.clone(),
        casm_hash_computation_data_proving_gas: casm_hash_computation_data_proving_gas.clone(),
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
    expected_bouncer
        .casm_hash_computation_data_proving_gas
        .extend(casm_hash_computation_data_proving_gas.clone());

    assert_eq!(updated_bouncer, expected_bouncer);
}

#[rstest]
#[case::sierra_gas_positive_flow(GasAmount(1), "ok")]
#[case::sierra_gas_block_full(GasAmount(11), "sierra_gas_block_full")]
#[case::proving_gas_positive_flow(GasAmount(0), "ok")]
#[case::proving_gas_block_full(GasAmount(0), "proving_gas_block_full")]
fn test_bouncer_try_update_gas_based(
    #[case] sierra_gas: GasAmount,
    #[case] scenario: &'static str,
) {
    let block_context = BlockContext::create_for_account_testing();
    let state = &mut test_state(&block_context.chain_info, Fee(0), &[]);
    let mut transactional_state = TransactionalState::create_transactional(state);
    let builtin_weights = BuiltinWeights::default();

    let range_check_count = 2;
    let max_capacity_builtin_counters =
        HashMap::from([(BuiltinName::range_check, range_check_count)]);
    let builtin_counters = match scenario {
        "proving_gas_block_full" => max_capacity_builtin_counters.clone(),
        // Use a minimal or empty map.
        "ok" | "sierra_gas_block_full" => {
            HashMap::from([(BuiltinName::range_check, range_check_count - 1)])
        }
        _ => panic!("Unexpected scenario: {}", scenario),
    };

    let proving_gas_max_capacity =
        builtin_weights.calc_proving_gas_from_builtin_counter(&max_capacity_builtin_counters);

    let block_max_capacity = BouncerWeights {
        l1_gas: 20,
        message_segment_length: 20,
        n_events: 20,
        state_diff_size: 20,
        n_txs: 20,
        sierra_gas: GasAmount(20),
        proving_gas: proving_gas_max_capacity,
    };
    let bouncer_config = BouncerConfig { block_max_capacity, builtin_weights };

    let accumulated_weights = BouncerWeights {
        l1_gas: 10,
        message_segment_length: 10,
        n_events: 10,
        state_diff_size: 10,
        sierra_gas: GasAmount(10),
        n_txs: 10,
        proving_gas: GasAmount(10),
    };

    let mut bouncer = Bouncer { accumulated_weights, bouncer_config, ..Bouncer::empty() };

    // Prepare the resources to be added to the bouncer.
    let execution_summary = ExecutionSummary { builtin_counters, ..Default::default() };
    let tx_resources = TransactionResources {
        computation: ComputationResources { sierra_gas, ..Default::default() },
        ..Default::default()
    };
    let tx_state_changes_keys = transactional_state.to_state_diff().unwrap().state_maps.keys();

    let result = bouncer.try_update(
        &transactional_state,
        &tx_state_changes_keys,
        &execution_summary,
        &tx_resources,
        &block_context.versioned_constants,
    );

    match scenario {
        "ok" => assert_matches!(result, Ok(())),
        "proving_gas_block_full" | "sierra_gas_block_full" => {
            assert_matches!(result, Err(TransactionExecutorError::BlockFull))
        }
        _ => panic!("Unexpected scenario: {}", scenario),
    }
}

#[test]
fn test_transaction_too_large_sierra_gas_based() {
    let block_context = BlockContext::create_for_account_testing();
    let mut state = test_state(&block_context.chain_info, Fee(0), &[]);
    let mut transactional_state = TransactionalState::create_transactional(&mut state);
    let block_max_capacity = BouncerWeights { sierra_gas: GasAmount(20), ..Default::default() };
    let bouncer_config =
        BouncerConfig { block_max_capacity, builtin_weights: BuiltinWeights::default() };

    // Use gas amount > block_max_capacity's.
    let exceeding_gas = GasAmount(30);
    let execution_summary = ExecutionSummary::default();
    let tx_resources = TransactionResources {
        computation: ComputationResources { sierra_gas: exceeding_gas, ..Default::default() },
        ..Default::default()
    };
    let tx_state_changes_keys = transactional_state.to_state_diff().unwrap().state_maps.keys();

    let result = verify_tx_weights_within_max_capacity(
        &transactional_state,
        &execution_summary,
        &tx_resources,
        &tx_state_changes_keys,
        &bouncer_config,
        &block_context.versioned_constants,
    )
    .map_err(TransactionExecutorError::TransactionExecutionError);

    let expected_weights = BouncerWeights {
        sierra_gas: exceeding_gas,
        n_txs: 1,
        proving_gas: exceeding_gas,
        ..BouncerWeights::empty()
    };

    assert_matches!(result, Err(
        TransactionExecutorError::TransactionExecutionError(
            TransactionExecutionError::TransactionTooLarge { max_capacity, tx_size }
        )
    )  if *max_capacity == bouncer_config.block_max_capacity && *tx_size == expected_weights);
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
        proving_gas: GasAmount(10),
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

/// This test verifies that `get_tx_weights` returns a reasonable casm hash computation data.
#[rstest]
fn test_get_tx_weights_with_casm_hash_computation(block_context: BlockContext) {
    // Set up state with declared contracts.
    let mut state_reader = DictStateReader::default();
    let test_contract_v0 = FeatureContract::TestContract(CairoVersion::Cairo0);
    let test_contract_v1 =
        FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));

    state_reader.add_class(&FeatureContractData::from(test_contract_v0));
    state_reader.add_class(&FeatureContractData::from(test_contract_v1));
    let state = CachedState::new(state_reader);

    let executed_class_hashes =
        HashSet::from([test_contract_v0.get_class_hash(), test_contract_v1.get_class_hash()]);

    // Call get_tx_weights.
    let result = get_tx_weights(
        &state,
        &executed_class_hashes,
        10, // n_visited_storage_entries
        &TransactionResources::default(),
        &StateMaps::default().keys(),
        &block_context.versioned_constants,
        &BuiltinCounterMap::default(),
        &BuiltinWeights::default(),
    );

    let tx_weights = result.unwrap();

    // Test that casm hash computation data keys equal executed class hashes
    let sierra_keys: HashSet<_> = tx_weights
        .casm_hash_computation_data_sierra_gas
        .class_hash_to_casm_hash_computation_gas
        .keys()
        .cloned()
        .collect();
    let proving_keys: HashSet<_> = tx_weights
        .casm_hash_computation_data_proving_gas
        .class_hash_to_casm_hash_computation_gas
        .keys()
        .cloned()
        .collect();

    assert_eq!(
        sierra_keys, executed_class_hashes,
        "Sierra gas keys should match executed class hashes"
    );
    assert_eq!(
        proving_keys, executed_class_hashes,
        "Proving gas keys should match executed class hashes"
    );

    // Verify gas amounts of casm hash computation data are positive.
    assert!(
        tx_weights
            .casm_hash_computation_data_sierra_gas
            .class_hash_to_casm_hash_computation_gas
            .values()
            .all(|&gas| gas > GasAmount::ZERO)
    );
    assert!(
        tx_weights
            .casm_hash_computation_data_proving_gas
            .class_hash_to_casm_hash_computation_gas
            .values()
            .all(|&gas| gas > GasAmount::ZERO)
    );

    // Test gas without casm hash computation is positive.
    assert!(
        tx_weights.casm_hash_computation_data_sierra_gas.gas_without_casm_hash_computation
            > GasAmount::ZERO
    );
    assert!(
        tx_weights.casm_hash_computation_data_proving_gas.gas_without_casm_hash_computation
            > GasAmount::ZERO
    );

    // Test that bouncer weights are equal to casm hash computation data total gas.
    let bouncer_weights = tx_weights.bouncer_weights;
    assert_eq!(
        bouncer_weights.sierra_gas,
        tx_weights.casm_hash_computation_data_sierra_gas.total_gas()
    );
    assert_eq!(
        bouncer_weights.proving_gas,
        tx_weights.casm_hash_computation_data_proving_gas.total_gas()
    );
}
