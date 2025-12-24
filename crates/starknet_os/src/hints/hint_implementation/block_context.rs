use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::insert_value_from_var_name;
use starknet_types_core::felt::Felt;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::error::OsHintResult;
use crate::hints::hint_implementation::execution::utils::set_state_entry;
use crate::hints::types::HintArgs;
use crate::hints::vars::{CairoStruct, Const, Ids};
use crate::vm_utils::insert_values_to_fields;

// Hint implementations.

pub(crate) fn guess_block_info<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let block_info = &hint_processor.get_current_execution_helper()?.os_block_input.block_info;
    let block_info_ptr = vm.add_memory_segment();
    insert_value_from_var_name(Ids::BlockInfo.into(), block_info_ptr, vm, ids_data, ap_tracking)?;
    insert_values_to_fields(
        block_info_ptr,
        CairoStruct::BlockInfo,
        vm,
        &[
            ("block_number", Felt::from(block_info.block_number.0).into()),
            ("block_timestamp", Felt::from(block_info.block_timestamp.0).into()),
            ("sequencer_address", (**block_info.sequencer_address).into()),
        ],
        hint_processor.program,
    )?;
    Ok(())
}

pub(crate) fn chain_id_and_fee_token_address<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let chain_info = &hint_processor.os_hints_config.chain_info;
    insert_value_from_var_name(
        Ids::ChainId.into(),
        Felt::try_from(&chain_info.chain_id)?,
        vm,
        ids_data,
        ap_tracking,
    )?;
    insert_value_from_var_name(
        Ids::FeeTokenAddress.into(),
        **chain_info.strk_fee_token_address,
        vm,
        ids_data,
        ap_tracking,
    )?;
    Ok(())
}

pub(crate) fn get_block_hash_mapping(
    HintArgs { ids_data, constants, vm, ap_tracking, exec_scopes, .. }: HintArgs<'_>,
) -> OsHintResult {
    let block_hash_contract_address = Const::BlockHashContractAddress.fetch(constants)?;
    set_state_entry(block_hash_contract_address, vm, exec_scopes, ids_data, ap_tracking)
}
