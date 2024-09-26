use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use starknet_api::executable_transaction::AccountTransaction as Transaction;
use starknet_api::execution_resources::GasVector;
use starknet_api::transaction::GasVectorComputationMode;

use super::fee_utils::get_vm_resources_cost;
use crate::abi::constants;
use crate::context::BlockContext;
use crate::fee::eth_gas_constants;
use crate::state::cached_state::StateChangesCount;
use crate::transaction::account_transaction::AccountTransaction;
use crate::utils::u64_from_usize;

#[cfg(test)]
#[path = "gas_usage_test.rs"]
pub mod test;

/// Returns the number of felts added to the output data availability segment as a result of adding
/// a transaction to a batch. Note that constant cells - such as the one that holds the number of
/// modified contracts - are not counted.
pub fn get_onchain_data_segment_length(state_changes_count: &StateChangesCount) -> usize {
    // TODO(Nimrod, 1/5/2024): Remove this function.

    // For each newly modified contract:
    // contract address (1 word).
    // + 1 word with the following info: A flag indicating whether the class hash was updated, the
    // number of entry updates, and the new nonce.
    let mut onchain_data_segment_length = state_changes_count.n_modified_contracts * 2;
    // For each class updated (through a deploy or a class replacement).
    onchain_data_segment_length +=
        state_changes_count.n_class_hash_updates * constants::CLASS_UPDATE_SIZE;
    // For each modified storage cell: key, new value.
    onchain_data_segment_length += state_changes_count.n_storage_updates * 2;
    // For each compiled class updated (through declare): class_hash, compiled_class_hash
    onchain_data_segment_length += state_changes_count.n_compiled_class_hash_updates * 2;

    onchain_data_segment_length
}

/// Returns the gas cost of data availability on L1.
pub fn get_da_gas_cost(state_changes_count: &StateChangesCount, use_kzg_da: bool) -> GasVector {
    let onchain_data_segment_length = get_onchain_data_segment_length(state_changes_count);

    let (l1_gas, blob_gas) = if use_kzg_da {
        (
            0_u8.into(),
            u64_from_usize(
                onchain_data_segment_length * eth_gas_constants::DATA_GAS_PER_FIELD_ELEMENT,
            )
            .into(),
        )
    } else {
        // TODO(Yoni, 1/5/2024): count the exact amount of nonzero bytes for each DA entry.
        let naive_cost = onchain_data_segment_length * eth_gas_constants::SHARP_GAS_PER_DA_WORD;

        // For each modified contract, the expected non-zeros bytes in the second word are:
        // 1 bytes for class hash flag; 2 for number of storage updates (up to 64K);
        // 3 for nonce update (up to 16M).
        let modified_contract_cost = eth_gas_constants::get_calldata_word_cost(1 + 2 + 3);
        let modified_contract_discount =
            eth_gas_constants::GAS_PER_MEMORY_WORD - modified_contract_cost;
        let mut discount = state_changes_count.n_modified_contracts * modified_contract_discount;

        // Up to balance of 8*(10**10) ETH.
        let fee_balance_value_cost = eth_gas_constants::get_calldata_word_cost(12);
        discount += eth_gas_constants::GAS_PER_MEMORY_WORD - fee_balance_value_cost;

        let gas = if naive_cost < discount {
            // Cost must be non-negative after discount.
            0
        } else {
            naive_cost - discount
        };

        (u64_from_usize(gas).into(), 0_u8.into())
    };

    GasVector { l1_gas, l1_data_gas: blob_gas, ..Default::default() }
}

/// Returns the number of felts added to the output messages segment as a result of adding
/// a transaction with the given parameters to a batch. Note that constant cells - such as the one
/// that holds the segment size - are not counted.
pub fn get_message_segment_length(
    l2_to_l1_payload_lengths: &[usize],
    l1_handler_payload_size: Option<usize>,
) -> usize {
    // Add L2-to-L1 message segment length; for each message, the OS outputs the following:
    // to_address, from_address, payload_size, payload.
    let mut message_segment_length = l2_to_l1_payload_lengths
        .iter()
        .map(|payload_length| constants::L2_TO_L1_MSG_HEADER_SIZE + payload_length)
        .sum();

    if let Some(payload_size) = l1_handler_payload_size {
        // The corresponding transaction is of type L1 handler; add the length of the L1-to-L2
        // message sent by the sequencer (that will be outputted by the OS), which is of the
        // following format: from_address=calldata[0], to_address=contract_address,
        // nonce, selector, payload_size, payload=calldata[1:].
        message_segment_length += constants::L1_TO_L2_MSG_HEADER_SIZE + payload_size;
    }

    message_segment_length
}

/// Returns the cost of ConsumedMessageToL2 event emissions caused by an L1 handler with the given
/// payload size.
pub fn get_consumed_message_to_l2_emissions_cost(
    l1_handler_payload_size: Option<usize>,
) -> GasVector {
    match l1_handler_payload_size {
        // The corresponding transaction is not an L1 handler.,
        None => GasVector::default(),
        Some(l1_handler_payload_size) => {
            get_event_emission_cost(
                constants::CONSUMED_MSG_TO_L2_N_TOPICS,
                // We're assuming the existence of one (not indexed) payload array.
                constants::CONSUMED_MSG_TO_L2_ENCODED_DATA_SIZE + l1_handler_payload_size,
            )
        }
    }
}

/// Returns the cost of LogMessageToL1 event emissions caused by the given messages payload length.
pub fn get_log_message_to_l1_emissions_cost(l2_to_l1_payload_lengths: &[usize]) -> GasVector {
    l2_to_l1_payload_lengths
        .iter()
        .map(|length| {
            get_event_emission_cost(
                constants::LOG_MSG_TO_L1_N_TOPICS,
                // We're assuming the existence of one (not indexed) payload array.
                constants::LOG_MSG_TO_L1_ENCODED_DATA_SIZE + *length,
            )
        })
        .fold(GasVector::ZERO, |accumulator, cost| {
            accumulator.checked_add(cost).unwrap_or_else(|| {
                panic!(
                    "Overflow in message emission gas costs; attempted to add {accumulator:?} to \
                     {cost:?}"
                )
            })
        })
}

fn get_event_emission_cost(n_topics: usize, data_length: usize) -> GasVector {
    GasVector::from_l1_gas(
        u64_from_usize(
            eth_gas_constants::GAS_PER_LOG
                + (n_topics + constants::N_DEFAULT_TOPICS) * eth_gas_constants::GAS_PER_LOG_TOPIC
                + data_length * eth_gas_constants::GAS_PER_LOG_DATA_WORD,
        )
        .into(),
    )
}

/// Returns an estimated lower bound for the gas required by the given account transaction.
pub fn estimate_minimal_gas_vector(
    block_context: &BlockContext,
    tx: &AccountTransaction,
    gas_usage_vector_computation_mode: &GasVectorComputationMode,
) -> GasVector {
    // TODO(Dori, 1/8/2023): Give names to the constant VM step estimates and regression-test them.
    let BlockContext { block_info, versioned_constants, .. } = block_context;
    let state_changes_by_account_tx = match &tx.tx {
        // We consider the following state changes: sender balance update (storage update) + nonce
        // increment (contract modification) (we exclude the sequencer balance update and the ERC20
        // contract modification since it occurs for every tx).
        Transaction::Declare(_) => StateChangesCount {
            n_storage_updates: 1,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 1,
        },
        Transaction::Invoke(_) => StateChangesCount {
            n_storage_updates: 1,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 1,
        },
        // DeployAccount also updates the address -> class hash mapping.
        Transaction::DeployAccount(_) => StateChangesCount {
            n_storage_updates: 1,
            n_class_hash_updates: 1,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 1,
        },
    };

    let data_segment_length = get_onchain_data_segment_length(&state_changes_by_account_tx);
    let os_steps_for_type =
        versioned_constants.os_resources_for_tx_type(&tx.tx_type(), tx.calldata_length()).n_steps
            + versioned_constants.os_kzg_da_resources(data_segment_length).n_steps;

    let resources = ExecutionResources { n_steps: os_steps_for_type, ..Default::default() };
    let da_gas_cost = get_da_gas_cost(&state_changes_by_account_tx, block_info.use_kzg_da);
    let vm_resources_cost = get_vm_resources_cost(
        versioned_constants,
        &resources,
        0,
        gas_usage_vector_computation_mode,
    );
    da_gas_cost.checked_add(vm_resources_cost).unwrap_or_else(|| {
        panic!(
            "Overflow in minimal gas estimation; attempted to add {da_gas_cost:?} to \
             {vm_resources_cost:?}"
        )
    })
}
