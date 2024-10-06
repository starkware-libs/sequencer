use rstest::{fixture, rstest};
use starknet_api::transaction::{GasVectorComputationMode, L2ToL1Payload};
use starknet_api::{invoke_tx_args, nonce};
use starknet_types_core::felt::Felt;

use crate::context::BlockContext;
use crate::execution::call_info::{
    CallExecution,
    CallInfo,
    ExecutionSummary,
    MessageToL1,
    OrderedL2ToL1Message,
};
use crate::fee::eth_gas_constants;
use crate::fee::gas_usage::{
    get_consumed_message_to_l2_emissions_cost,
    get_log_message_to_l1_emissions_cost,
    get_message_segment_length,
};
use crate::fee::resources::{GasVector, StarknetResources, StateResources};
use crate::state::cached_state::StateChangesCount;
use crate::test_utils::contracts::FeatureContract;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::{create_calldata, create_trivial_calldata, CairoVersion, BALANCE};
use crate::transaction::constants;
use crate::transaction::objects::HasRelatedFeeType;
use crate::transaction::test_utils::{
    account_invoke_tx,
    calculate_class_info_for_testing,
    create_resource_bounds,
};
use crate::transaction::transactions::ExecutableTransaction;
use crate::utils::{u64_from_usize, usize_from_u64};
use crate::versioned_constants::VersionedConstants;

#[fixture]
fn versioned_constants() -> &'static VersionedConstants {
    VersionedConstants::latest_constants()
}

/// This test goes over seven cases. In each case, we calculate the gas usage given the parameters.
/// We then perform the same calculation manually, each time using only the relevant parameters.
/// The seven cases are:
///     1. An empty transaction.
///     2. A Declare transaction.
///     3. A DeployAccount transaction.
///     4. An L1 handler.
///     5. A transaction with L2-to-L1 messages.
///     6. A transaction that modifies the storage.
///     7. A combination of cases 4. 5. and 6.
// TODO(Aner, 29/01/24) Refactor with assert on GasVector objects.
// TODO(Aner, 29/01/24) Refactor to replace match with if when formatting is nicer
#[rstest]
fn test_calculate_tx_gas_usage_basic<'a>(
    #[values(false, true)] use_kzg_da: bool,
    #[values(GasVectorComputationMode::NoL2Gas, GasVectorComputationMode::All)]
    gas_vector_computation_mode: GasVectorComputationMode,
) {
    // An empty transaction (a theoretical case for sanity check).
    let versioned_constants = VersionedConstants::create_for_account_testing();
    let empty_tx_starknet_resources = StarknetResources::default();
    let empty_tx_gas_usage_vector = empty_tx_starknet_resources.to_gas_vector(
        &versioned_constants,
        use_kzg_da,
        &gas_vector_computation_mode,
    );
    assert_eq!(empty_tx_gas_usage_vector, GasVector::default());

    // Declare.
    for cairo_version in [CairoVersion::Cairo0, CairoVersion::Cairo1] {
        let empty_contract = FeatureContract::Empty(cairo_version).get_class();
        let class_info = calculate_class_info_for_testing(empty_contract);
        let declare_tx_starknet_resources = StarknetResources::new(
            0,
            0,
            class_info.code_size(),
            StateResources::default(),
            None,
            ExecutionSummary::default(),
        );
        let gas_per_code_byte = versioned_constants
            .get_archival_data_gas_costs(&gas_vector_computation_mode)
            .gas_per_code_byte;
        let code_gas_cost = (gas_per_code_byte
            * u64_from_usize(
                (class_info.bytecode_length() + class_info.sierra_program_length())
                    * eth_gas_constants::WORD_WIDTH
                    + class_info.abi_length(),
            ))
        .to_integer()
        .into();
        let manual_gas_vector = match gas_vector_computation_mode {
            GasVectorComputationMode::NoL2Gas => GasVector::from_l1_gas(code_gas_cost),
            GasVectorComputationMode::All => GasVector::from_l2_gas(code_gas_cost),
        };
        let declare_gas_usage_vector = declare_tx_starknet_resources.to_gas_vector(
            &versioned_constants,
            use_kzg_da,
            &gas_vector_computation_mode,
        );
        assert_eq!(manual_gas_vector, declare_gas_usage_vector);
    }

    // DeployAccount.

    let deploy_account_state_changes_count = StateChangesCount {
        n_storage_updates: 0,
        n_class_hash_updates: 1,
        n_compiled_class_hash_updates: 0,
        n_modified_contracts: 1,
    };

    // Manual calculation.
    let calldata_length = 0;
    let signature_length = 2;
    let deploy_account_tx_starknet_resources = StarknetResources::new(
        calldata_length,
        signature_length,
        0,
        StateResources::new_for_testing(deploy_account_state_changes_count),
        None,
        ExecutionSummary::default(),
    );
    let gas_per_data_felt = versioned_constants
        .get_archival_data_gas_costs(&gas_vector_computation_mode)
        .gas_per_data_felt;
    let calldata_and_signature_gas_cost = (gas_per_data_felt
        * u64_from_usize(calldata_length + signature_length))
    .to_integer()
    .into();
    let manual_starknet_gas_usage_vector = match gas_vector_computation_mode {
        GasVectorComputationMode::NoL2Gas => {
            GasVector::from_l1_gas(calldata_and_signature_gas_cost)
        }
        GasVectorComputationMode::All => GasVector::from_l2_gas(calldata_and_signature_gas_cost),
    };
    let manual_gas_vector = manual_starknet_gas_usage_vector
        + deploy_account_tx_starknet_resources.state.to_gas_vector(use_kzg_da);

    let deploy_account_gas_usage_vector = deploy_account_tx_starknet_resources.to_gas_vector(
        &versioned_constants,
        use_kzg_da,
        &gas_vector_computation_mode,
    );
    assert_eq!(manual_gas_vector, deploy_account_gas_usage_vector);

    // L1 handler.

    let l1_handler_payload_size = 4;
    let l1_handler_tx_starknet_resources = StarknetResources::new(
        l1_handler_payload_size,
        signature_length,
        0,
        StateResources::default(),
        Some(l1_handler_payload_size),
        ExecutionSummary::default(),
    );
    let l1_handler_gas_usage_vector = l1_handler_tx_starknet_resources.to_gas_vector(
        &versioned_constants,
        use_kzg_da,
        &gas_vector_computation_mode,
    );

    // Manual calculation.
    let message_segment_length = get_message_segment_length(&[], Some(l1_handler_payload_size));
    let calldata_and_signature_gas_cost = (gas_per_data_felt
        * u64_from_usize(l1_handler_payload_size + signature_length))
    .to_integer()
    .into();
    let calldata_and_signature_gas_cost_vector = match gas_vector_computation_mode {
        GasVectorComputationMode::NoL2Gas => {
            GasVector::from_l1_gas(calldata_and_signature_gas_cost)
        }
        GasVectorComputationMode::All => GasVector::from_l2_gas(calldata_and_signature_gas_cost),
    };
    let manual_starknet_l1_gas_usage = message_segment_length
        * eth_gas_constants::GAS_PER_MEMORY_WORD
        + eth_gas_constants::GAS_PER_COUNTER_DECREASE
        + usize_from_u64(
            get_consumed_message_to_l2_emissions_cost(Some(l1_handler_payload_size)).l1_gas.0,
        )
        .unwrap();
    let manual_starknet_l1_gas_usage_vector =
        GasVector::from_l1_gas(u64_from_usize(manual_starknet_l1_gas_usage).into());
    let manual_sharp_gas_usage =
        message_segment_length * eth_gas_constants::SHARP_GAS_PER_MEMORY_WORD;
    let manual_gas_computation =
        GasVector::from_l1_gas(u64_from_usize(manual_sharp_gas_usage).into())
            + manual_starknet_l1_gas_usage_vector
            + calldata_and_signature_gas_cost_vector;
    assert_eq!(l1_handler_gas_usage_vector, manual_gas_computation);

    // Any transaction with L2-to-L1 messages.

    let mut call_infos = Vec::new();
    let mut l2_to_l1_payload_lengths = vec![];
    for i in 0..4 {
        let payload_vec = vec![Felt::ZERO; i];
        l2_to_l1_payload_lengths.push(payload_vec.len());

        let call_info = CallInfo {
            execution: CallExecution {
                l2_to_l1_messages: vec![OrderedL2ToL1Message {
                    message: MessageToL1 {
                        payload: L2ToL1Payload(payload_vec),
                        ..Default::default()
                    },
                    ..Default::default()
                }],
                ..Default::default()
            },
            ..Default::default()
        }
        .with_some_class_hash();

        call_infos.push(call_info);
    }
    let execution_summary = CallInfo::summarize_many(call_infos.iter());

    let l2_to_l1_state_changes_count = StateChangesCount {
        n_storage_updates: 0,
        n_class_hash_updates: 0,
        n_compiled_class_hash_updates: 0,
        n_modified_contracts: 1,
    };
    let l2_to_l1_starknet_resources = StarknetResources::new(
        0,
        0,
        0,
        StateResources::new_for_testing(l2_to_l1_state_changes_count),
        None,
        execution_summary.clone(),
    );

    let l2_to_l1_messages_gas_usage_vector = l2_to_l1_starknet_resources.to_gas_vector(
        &versioned_constants,
        use_kzg_da,
        &gas_vector_computation_mode,
    );

    // Manual calculation.
    // No L2 gas is used, so gas amount does not depend on gas vector computation mode.
    let message_segment_length = get_message_segment_length(&l2_to_l1_payload_lengths, None);
    let n_l2_to_l1_messages = l2_to_l1_payload_lengths.len();
    let manual_starknet_gas_usage = message_segment_length * eth_gas_constants::GAS_PER_MEMORY_WORD
        + n_l2_to_l1_messages * eth_gas_constants::GAS_PER_ZERO_TO_NONZERO_STORAGE_SET
        + usize_from_u64(get_log_message_to_l1_emissions_cost(&l2_to_l1_payload_lengths).l1_gas.0)
            .unwrap();
    let manual_sharp_gas_usage = message_segment_length
        * eth_gas_constants::SHARP_GAS_PER_MEMORY_WORD
        + usize_from_u64(l2_to_l1_starknet_resources.state.to_gas_vector(use_kzg_da).l1_gas.0)
            .unwrap();
    let manual_sharp_blob_gas_usage =
        l2_to_l1_starknet_resources.state.to_gas_vector(use_kzg_da).l1_data_gas;
    let manual_gas_computation = GasVector {
        l1_gas: u64_from_usize(manual_starknet_gas_usage + manual_sharp_gas_usage).into(),
        l1_data_gas: manual_sharp_blob_gas_usage,
        ..Default::default()
    };

    assert_eq!(l2_to_l1_messages_gas_usage_vector, manual_gas_computation);

    // Any calculation with storage writings.t

    let n_modified_contracts = 7;
    let n_storage_updates = 11;
    let storage_writes_state_changes_count = StateChangesCount {
        n_storage_updates,
        n_class_hash_updates: 0,
        n_compiled_class_hash_updates: 0,
        n_modified_contracts,
    };
    let storage_writes_starknet_resources = StarknetResources::new(
        0,
        0,
        0,
        StateResources::new_for_testing(storage_writes_state_changes_count),
        None,
        ExecutionSummary::default(),
    );

    let storage_writings_gas_usage_vector = storage_writes_starknet_resources.to_gas_vector(
        &versioned_constants,
        use_kzg_da,
        &gas_vector_computation_mode,
    );

    // Manual calculation.
    // No L2 gas is used, so gas amount does not depend on gas vector computation mode.
    let manual_gas_computation = storage_writes_starknet_resources.state.to_gas_vector(use_kzg_da);

    assert_eq!(manual_gas_computation, storage_writings_gas_usage_vector);

    // Combined case of an L1 handler, L2-to-L1 messages and storage writes.
    let combined_state_changes_count = StateChangesCount {
        n_storage_updates: storage_writes_state_changes_count.n_storage_updates,
        n_class_hash_updates: 0,
        n_compiled_class_hash_updates: 0,
        n_modified_contracts: storage_writes_state_changes_count.n_modified_contracts
            + l2_to_l1_state_changes_count.n_modified_contracts,
    };
    let combined_cases_starknet_resources = StarknetResources::new(
        l1_handler_payload_size,
        signature_length,
        0,
        StateResources::new_for_testing(combined_state_changes_count),
        Some(l1_handler_payload_size),
        execution_summary.clone(),
    );

    let gas_usage_vector = combined_cases_starknet_resources.to_gas_vector(
        &versioned_constants,
        use_kzg_da,
        &gas_vector_computation_mode,
    );

    // Manual calculation.
    let fee_balance_discount = match use_kzg_da {
        true => 0,
        false => {
            eth_gas_constants::GAS_PER_MEMORY_WORD - eth_gas_constants::get_calldata_word_cost(12)
        }
    };

    let expected_gas_vector = GasVector {
        l1_gas: l1_handler_gas_usage_vector.l1_gas
        + l2_to_l1_messages_gas_usage_vector.l1_gas
        + storage_writings_gas_usage_vector.l1_gas
        // l2_to_l1_messages_gas_usage and storage_writings_gas_usage got a discount each, while
        // the combined calculation got it once.
        + u64_from_usize(fee_balance_discount).into(),
        // Expected blob gas usage is from data availability only.
        l1_data_gas: combined_cases_starknet_resources.state.to_gas_vector(use_kzg_da).l1_data_gas,
        l2_gas: l1_handler_gas_usage_vector.l2_gas,
    };

    assert_eq!(expected_gas_vector, gas_usage_vector);
}

// Test that we exclude the fee token contract modification and adds the account’s balance change
// in the state changes.
#[rstest]
fn test_calculate_tx_gas_usage(
    #[values(false, true)] use_kzg_da: bool,
    #[values(GasVectorComputationMode::NoL2Gas, GasVectorComputationMode::All)]
    gas_vector_computation_mode: GasVectorComputationMode,
) {
    let max_resource_bounds = create_resource_bounds(&gas_vector_computation_mode);
    let account_cairo_version = CairoVersion::Cairo0;
    let test_contract_cairo_version = CairoVersion::Cairo0;
    let block_context = &BlockContext::create_for_account_testing_with_kzg(use_kzg_da);
    let versioned_constants = &block_context.versioned_constants;
    let chain_info = &block_context.chain_info;
    let account_contract = FeatureContract::AccountWithoutValidations(account_cairo_version);
    let test_contract = FeatureContract::TestContract(test_contract_cairo_version);
    let account_contract_address = account_contract.get_instance_address(0);
    let state = &mut test_state(chain_info, BALANCE, &[(account_contract, 1), (test_contract, 1)]);

    let account_tx = account_invoke_tx(invoke_tx_args! {
            sender_address: account_contract_address,
            calldata: create_trivial_calldata(test_contract.get_instance_address(0)),
            resource_bounds: max_resource_bounds,
    });
    let calldata_length = account_tx.calldata_length();
    let signature_length = account_tx.signature_length();
    let fee_token_address = chain_info.fee_token_address(&account_tx.fee_type());
    let tx_execution_info = account_tx.execute(state, block_context, true, true).unwrap();

    let n_storage_updates = 1; // For the account balance update.
    let n_modified_contracts = 1;
    let state_changes_count = StateChangesCount {
        n_storage_updates,
        n_class_hash_updates: 0,
        n_modified_contracts,
        n_compiled_class_hash_updates: 0,
    };
    let starknet_resources = StarknetResources::new(
        calldata_length,
        signature_length,
        0,
        StateResources::new_for_testing(state_changes_count),
        None,
        ExecutionSummary::default(),
    );

    assert_eq!(
        starknet_resources.to_gas_vector(
            versioned_constants,
            use_kzg_da,
            &gas_vector_computation_mode
        ),
        tx_execution_info.receipt.resources.starknet_resources.to_gas_vector(
            versioned_constants,
            use_kzg_da,
            &gas_vector_computation_mode
        )
    );

    // A tx that changes the account and some other balance in execute.
    let some_other_account_address = account_contract.get_instance_address(17);
    let execute_calldata = create_calldata(
        fee_token_address,
        constants::TRANSFER_ENTRY_POINT_NAME,
        &[
            *some_other_account_address.0.key(), // Calldata: recipient.
            Felt::TWO,                           // Calldata: lsb amount.
            Felt::ZERO,                          // Calldata: msb amount.
        ],
    );

    let account_tx = account_invoke_tx(invoke_tx_args! {
        resource_bounds: max_resource_bounds,
        sender_address: account_contract_address,
        calldata: execute_calldata,
        nonce: nonce!(1_u8),
    });

    let calldata_length = account_tx.calldata_length();
    let signature_length = account_tx.signature_length();
    let tx_execution_info = account_tx.execute(state, block_context, true, true).unwrap();
    // For the balance update of the sender and the recipient.
    let n_storage_updates = 2;
    // Only the account contract modification (nonce update) excluding the fee token contract.
    let n_modified_contracts = 1;
    let state_changes_count = StateChangesCount {
        n_storage_updates,
        n_class_hash_updates: 0,
        n_modified_contracts,
        n_compiled_class_hash_updates: 0,
    };
    let execution_call_info =
        &tx_execution_info.execute_call_info.expect("Execution call info should exist.");
    let execution_summary = CallInfo::summarize_many(vec![execution_call_info].into_iter());
    let starknet_resources = StarknetResources::new(
        calldata_length,
        signature_length,
        0,
        StateResources::new_for_testing(state_changes_count),
        None,
        // The transfer entrypoint emits an event - pass the call info to count its resources.
        execution_summary,
    );

    assert_eq!(
        starknet_resources.to_gas_vector(
            versioned_constants,
            use_kzg_da,
            &gas_vector_computation_mode
        ),
        tx_execution_info.receipt.resources.starknet_resources.to_gas_vector(
            versioned_constants,
            use_kzg_da,
            &gas_vector_computation_mode
        )
    );
}
