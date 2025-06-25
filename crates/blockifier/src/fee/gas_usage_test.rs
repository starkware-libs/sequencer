use std::sync::Arc;

use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use num_rational::Ratio;
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use starknet_api::block::{FeeType, StarknetVersion};
use starknet_api::execution_resources::{GasAmount, GasVector};
use starknet_api::invoke_tx_args;
use starknet_api::test_utils::{DEFAULT_ETH_L1_DATA_GAS_PRICE, DEFAULT_ETH_L1_GAS_PRICE};
use starknet_api::transaction::fields::GasVectorComputationMode;
use starknet_api::transaction::{EventContent, EventData, EventKey};
use starknet_types_core::felt::Felt;

use crate::abi::constants;
use crate::blockifier_versioned_constants::{ResourceCost, VersionedConstants, VmResourceCosts};
use crate::context::BlockContext;
use crate::execution::call_info::{CallExecution, CallInfo, OrderedEvent};
use crate::fee::eth_gas_constants;
use crate::fee::fee_utils::{get_fee_by_gas_vector, GasVectorToL1GasForFee};
use crate::fee::gas_usage::{get_da_gas_cost, get_message_segment_length};
use crate::fee::resources::{
    ComputationResources,
    StarknetResources,
    StateResources,
    TransactionResources,
};
use crate::state::cached_state::StateChangesCount;
use crate::test_utils::get_vm_resource_usage;
use crate::transaction::test_utils::invoke_tx_with_default_flags;
use crate::utils::u64_from_usize;

pub fn create_event_for_testing(keys_size: usize, data_size: usize) -> OrderedEvent {
    OrderedEvent {
        order: 0,
        event: EventContent {
            keys: vec![EventKey(Felt::ZERO); keys_size],
            data: EventData(vec![Felt::ZERO; data_size]),
        },
    }
}

#[fixture]
fn versioned_constants() -> &'static VersionedConstants {
    VersionedConstants::latest_constants()
}

// Starknet resources with many resources (of arbitrary values) for testing.
#[fixture]
fn starknet_resources() -> StarknetResources {
    let call_info_1 = CallInfo {
        execution: CallExecution {
            events: vec![create_event_for_testing(1, 2), create_event_for_testing(1, 2)],
            ..Default::default()
        },
        ..Default::default()
    };
    let call_info_2 = CallInfo {
        execution: CallExecution {
            events: vec![create_event_for_testing(1, 0), create_event_for_testing(0, 1)],
            ..Default::default()
        },
        ..Default::default()
    };
    let call_infos: Vec<CallInfo> = vec![call_info_1, call_info_2]
        .into_iter()
        .map(|call_info| call_info.with_some_class_hash())
        .collect();
    let execution_summary =
        CallInfo::summarize_many(call_infos.iter(), VersionedConstants::latest_constants());
    let state_resources = StateResources::new_for_testing(
        StateChangesCount {
            n_storage_updates: 7,
            n_class_hash_updates: 11,
            n_compiled_class_hash_updates: 13,
            n_modified_contracts: 17,
        },
        19,
    );
    StarknetResources::new(2_usize, 3_usize, 4_usize, state_resources, 6.into(), execution_summary)
}

#[rstest]
fn test_get_event_gas_cost(
    versioned_constants: &VersionedConstants,
    #[values(false, true)] use_kzg_da: bool,
    #[values(GasVectorComputationMode::NoL2Gas, GasVectorComputationMode::All)]
    gas_vector_computation_mode: GasVectorComputationMode,
) {
    let archival_gas_costs =
        versioned_constants.get_archival_data_gas_costs(&gas_vector_computation_mode);
    let (event_key_factor, data_word_cost) =
        (archival_gas_costs.event_key_factor, archival_gas_costs.gas_per_data_felt);
    let call_infos: Vec<CallInfo> =
        vec![CallInfo::default(), CallInfo::default(), CallInfo::default()]
            .into_iter()
            .map(|call_info| call_info.with_some_class_hash())
            .collect();
    let execution_summary = CallInfo::summarize_many(call_infos.iter(), versioned_constants);
    let starknet_resources =
        StarknetResources::new(0, 0, 0, StateResources::default(), None, execution_summary);
    assert_eq!(
        GasVector::default(),
        starknet_resources.to_gas_vector(
            versioned_constants,
            use_kzg_da,
            &gas_vector_computation_mode
        )
    );

    let call_info_1 = CallInfo {
        execution: CallExecution {
            events: vec![create_event_for_testing(1, 2), create_event_for_testing(1, 2)],
            ..Default::default()
        },
        ..Default::default()
    };
    let call_info_2 = CallInfo {
        execution: CallExecution {
            events: vec![create_event_for_testing(1, 0), create_event_for_testing(0, 1)],
            ..Default::default()
        },
        ..Default::default()
    };
    let call_info_3 = CallInfo {
        execution: CallExecution {
            events: vec![create_event_for_testing(0, 1)],
            ..Default::default()
        },
        inner_calls: vec![
            CallInfo {
                execution: CallExecution {
                    events: vec![create_event_for_testing(5, 5)],
                    ..Default::default()
                },
                ..Default::default()
            }
            .with_some_class_hash(),
        ],
        ..Default::default()
    };
    let call_infos: Vec<CallInfo> = vec![call_info_1, call_info_2, call_info_3]
        .into_iter()
        .map(|call_info| call_info.with_some_class_hash())
        .collect();
    let execution_summary = CallInfo::summarize_many(call_infos.iter(), versioned_constants);
    // 8 keys and 11 data words overall.
    let expected_gas = (data_word_cost * (event_key_factor * 8_u64 + 11_u64)).to_integer().into();
    let expected_gas_vector = match gas_vector_computation_mode {
        GasVectorComputationMode::NoL2Gas => GasVector::from_l1_gas(expected_gas),
        GasVectorComputationMode::All => GasVector::from_l2_gas(expected_gas),
    };
    let starknet_resources =
        StarknetResources::new(0, 0, 0, StateResources::default(), None, execution_summary);
    let gas_vector = starknet_resources.to_gas_vector(
        versioned_constants,
        use_kzg_da,
        &gas_vector_computation_mode,
    );
    assert_eq!(expected_gas_vector, gas_vector);
    assert_ne!(GasVector::default(), gas_vector)
}

#[rstest]
#[case::storage_write(StateChangesCount {
    n_storage_updates: 1,
    n_class_hash_updates:0,
    n_compiled_class_hash_updates:0,
    n_modified_contracts:0,
})
]
#[case::deploy_account(StateChangesCount {
    n_storage_updates: 0,
    n_class_hash_updates:1,
    n_compiled_class_hash_updates:0,
    n_modified_contracts:1,
})
]
#[case::declare(StateChangesCount {
    n_storage_updates: 0,
    n_class_hash_updates:0,
    n_compiled_class_hash_updates:1,
    n_modified_contracts:0,
})
]
#[case::general_scenario(StateChangesCount {
    n_storage_updates: 7,
    n_class_hash_updates:11,
    n_compiled_class_hash_updates:13,
    n_modified_contracts:17,
})
]
fn test_get_da_gas_cost_basic(#[case] state_changes_count: StateChangesCount) {
    // Manual calculation.
    let on_chain_data_segment_length = state_changes_count.n_storage_updates * 2
        + state_changes_count.n_class_hash_updates
        + state_changes_count.n_compiled_class_hash_updates * 2
        + state_changes_count.n_modified_contracts * 2;
    let manual_blob_gas_usage =
        on_chain_data_segment_length * eth_gas_constants::DATA_GAS_PER_FIELD_ELEMENT;

    let computed_gas_vector = get_da_gas_cost(&state_changes_count, true);
    assert_eq!(
        GasVector::from_l1_data_gas(u64_from_usize(manual_blob_gas_usage).into()),
        computed_gas_vector
    );
}

#[test]
fn test_onchain_data_discount() {
    let use_kzg_da = false;
    // Check that there's no negative cost.
    assert_eq!(get_da_gas_cost(&StateChangesCount::default(), use_kzg_da).l1_gas, 0_u8.into());

    // Check discount: modified_contract_felt and fee balance discount.
    let state_changes_count = StateChangesCount {
        // Fee balance update.
        n_storage_updates: 1,
        n_modified_contracts: 7,
        ..StateChangesCount::default()
    };

    let modified_contract_calldata_cost = 6 * eth_gas_constants::GAS_PER_MEMORY_BYTE
        + 26 * eth_gas_constants::GAS_PER_MEMORY_ZERO_BYTE;
    let modified_contract_cost = modified_contract_calldata_cost
        + eth_gas_constants::SHARP_ADDITIONAL_GAS_PER_MEMORY_WORD
        - eth_gas_constants::DISCOUNT_PER_DA_WORD;
    let contract_address_cost = eth_gas_constants::SHARP_GAS_PER_DA_WORD;

    let fee_balance_value_calldata_cost = 12 * eth_gas_constants::GAS_PER_MEMORY_BYTE
        + 20 * eth_gas_constants::GAS_PER_MEMORY_ZERO_BYTE;
    let fee_balance_value_cost = fee_balance_value_calldata_cost
        + eth_gas_constants::SHARP_ADDITIONAL_GAS_PER_MEMORY_WORD
        - eth_gas_constants::DISCOUNT_PER_DA_WORD;
    let fee_balance_key_cost = eth_gas_constants::SHARP_GAS_PER_DA_WORD;

    let expected_cost = state_changes_count.n_modified_contracts
        * (contract_address_cost + modified_contract_cost)
        + fee_balance_key_cost
        + fee_balance_value_cost;

    assert_eq!(
        get_da_gas_cost(&state_changes_count, use_kzg_da).l1_gas,
        u64::try_from(expected_cost).unwrap().into()
    );

    // Test 10% discount.
    let state_changes_count =
        StateChangesCount { n_storage_updates: 27, ..StateChangesCount::default() };

    let cost_without_discount = (state_changes_count.n_storage_updates * 2) * (512 + 100);
    let actual_cost = get_da_gas_cost(&state_changes_count, use_kzg_da).l1_gas;
    let cost_ratio = ResourceCost::new(actual_cost.0, u64_from_usize(cost_without_discount));
    assert!(cost_ratio <= ResourceCost::new(9, 10));
    assert!(cost_ratio >= ResourceCost::new(88, 100));
}

#[rstest]
#[case(vec![10, 20, 30], Some(50))]
#[case(vec![10, 20, 30], None)]
#[case(vec![], Some(50))]
#[case(vec![], None)]
fn test_get_message_segment_length(
    #[case] l2_to_l1_payload_lengths: Vec<usize>,
    #[case] l1_handler_payload_size: Option<usize>,
) {
    let result = get_message_segment_length(&l2_to_l1_payload_lengths, l1_handler_payload_size);

    let expected_result: usize = l2_to_l1_payload_lengths.len()
        * constants::L2_TO_L1_MSG_HEADER_SIZE
        + l2_to_l1_payload_lengths.iter().sum::<usize>()
        + if let Some(size) = l1_handler_payload_size {
            constants::L1_TO_L2_MSG_HEADER_SIZE + size
        } else {
            0
        };

    assert_eq!(result, expected_result);
}

#[rstest]
fn test_discounted_gas_from_gas_vector_computation() {
    let tx_context = BlockContext::create_for_testing()
        .to_tx_context(&invoke_tx_with_default_flags(invoke_tx_args! {}));
    let mut gas_usage =
        GasVector { l1_gas: 100_u8.into(), l1_data_gas: 2_u8.into(), l2_gas: 3_u8.into() };
    let actual_result = gas_usage.to_l1_gas_for_fee(
        tx_context.get_gas_prices(),
        &tx_context.block_context.versioned_constants,
    );
    let converted_l2_gas = tx_context
        .block_context
        .versioned_constants
        .sierra_gas_to_l1_gas_amount_round_up(gas_usage.l2_gas);

    let result_div_ceil = gas_usage.l1_gas
        + (gas_usage.l1_data_gas.checked_mul(DEFAULT_ETH_L1_DATA_GAS_PRICE.into()).unwrap())
            .checked_div_ceil(DEFAULT_ETH_L1_GAS_PRICE)
            .unwrap()
        + converted_l2_gas;
    let result_div_floor = gas_usage.l1_gas
        + (gas_usage.l1_data_gas.checked_mul(DEFAULT_ETH_L1_DATA_GAS_PRICE.into()).unwrap())
            .checked_div(DEFAULT_ETH_L1_GAS_PRICE)
            .unwrap()
        + converted_l2_gas;

    assert_eq!(actual_result, result_div_ceil);
    assert_eq!(actual_result, result_div_floor + 1_u8.into());
    assert!(
        get_fee_by_gas_vector(
            &tx_context.block_context.block_info,
            gas_usage,
            &FeeType::Eth,
            tx_context.effective_tip()
        ) <= actual_result.checked_mul(DEFAULT_ETH_L1_GAS_PRICE.into()).unwrap()
    );

    // Make sure L2 gas has an effect.
    gas_usage.l2_gas = 0_u8.into();
    assert!(
        gas_usage.to_l1_gas_for_fee(
            tx_context.get_gas_prices(),
            &tx_context.block_context.versioned_constants,
        ) < actual_result
    );
}

#[rstest]
// Assert gas computation results are as expected. The goal of this test is to prevent unwanted
// changes to the gas computation.
fn test_gas_computation_regression_test(
    starknet_resources: StarknetResources,
    #[values(false, true)] use_kzg_da: bool,
    #[values(GasVectorComputationMode::NoL2Gas, GasVectorComputationMode::All)]
    gas_vector_computation_mode: GasVectorComputationMode,
) {
    // Use a constant version of the versioned constants so that version changes do not break this
    // test. This specific version is arbitrary.
    // TODO(Amos, 1/10/2024): Parameterize the version.
    let mut versioned_constants =
        VersionedConstants::get(&StarknetVersion::V0_13_2_1).unwrap().clone();

    // Change the VM resource fee cost so that the L2 / L1 gas ratio is a fraction.
    let vm_resource_fee_cost = VmResourceCosts {
        builtins: versioned_constants.vm_resource_fee_cost.builtins.clone(),
        n_steps: Ratio::new(30, 10000),
    };
    versioned_constants.vm_resource_fee_cost = Arc::new(vm_resource_fee_cost);

    // Test Starknet resources.
    let actual_starknet_resources_gas_vector = starknet_resources.to_gas_vector(
        &versioned_constants,
        use_kzg_da,
        &gas_vector_computation_mode,
    );
    let expected_starknet_resources_gas_vector = match gas_vector_computation_mode {
        GasVectorComputationMode::NoL2Gas => match use_kzg_da {
            true => GasVector {
                l1_gas: GasAmount(21544),
                l1_data_gas: GasAmount(2720),
                l2_gas: GasAmount(0),
            },
            false => GasVector::from_l1_gas(GasAmount(62835)),
        },
        GasVectorComputationMode::All => match use_kzg_da {
            true => GasVector {
                l1_gas: GasAmount(21543),
                l1_data_gas: GasAmount(2720),
                l2_gas: GasAmount(87040),
            },
            false => GasVector {
                l1_gas: GasAmount(62834),
                l1_data_gas: GasAmount(0),
                l2_gas: GasAmount(87040),
            },
        },
    };
    assert_eq!(
        actual_starknet_resources_gas_vector, expected_starknet_resources_gas_vector,
        "Unexpected gas computation result for starknet resources. If this is intentional please \
         fix this test."
    );

    // Test VM resources.
    let mut tx_vm_resources = get_vm_resource_usage();
    tx_vm_resources.n_memory_holes = 2;
    let n_reverted_steps = 15;
    let (sierra_gas, reverted_sierra_gas) = match gas_vector_computation_mode {
        GasVectorComputationMode::NoL2Gas => (GasAmount(0), GasAmount(0)),
        GasVectorComputationMode::All => (GasAmount(13), GasAmount(7)),
    };
    let computation_resources = ComputationResources {
        tx_vm_resources,
        os_vm_resources: ExecutionResources::default(),
        n_reverted_steps,
        sierra_gas,
        reverted_sierra_gas,
    };
    let actual_computation_resources_gas_vector =
        computation_resources.to_gas_vector(&versioned_constants, &gas_vector_computation_mode);
    let expected_computation_resources_gas_vector = match gas_vector_computation_mode {
        GasVectorComputationMode::NoL2Gas => GasVector::from_l1_gas(GasAmount(31)),
        GasVectorComputationMode::All => GasVector::from_l2_gas(GasAmount(1033354)),
    };
    assert_eq!(
        actual_computation_resources_gas_vector, expected_computation_resources_gas_vector,
        "Unexpected gas computation result for VM resources. If this is intentional please fix \
         this test."
    );

    // Test transaction resources
    let tx_resources =
        TransactionResources { starknet_resources, computation: computation_resources };
    let actual_gas_vector =
        tx_resources.to_gas_vector(&versioned_constants, use_kzg_da, &gas_vector_computation_mode);
    let expected_gas_vector = match gas_vector_computation_mode {
        GasVectorComputationMode::NoL2Gas => match use_kzg_da {
            true => GasVector {
                l1_gas: GasAmount(21575),
                l1_data_gas: GasAmount(2720),
                l2_gas: GasAmount(0),
            },
            false => GasVector::from_l1_gas(GasAmount(62866)),
        },
        GasVectorComputationMode::All => match use_kzg_da {
            true => GasVector {
                l1_gas: GasAmount(21543),
                l1_data_gas: GasAmount(2720),
                l2_gas: GasAmount(1120394),
            },
            false => GasVector {
                l1_gas: GasAmount(62834),
                l1_data_gas: GasAmount(0),
                l2_gas: GasAmount(1120394),
            },
        },
    };
    assert_eq!(
        actual_gas_vector, expected_gas_vector,
        "Unexpected gas computation result for tx resources. If this is intentional please fix \
         this test."
    );
}
