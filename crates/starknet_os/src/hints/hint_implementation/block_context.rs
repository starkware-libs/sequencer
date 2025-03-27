use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_ptr_from_var_name,
    insert_value_from_var_name,
    insert_value_into_ap,
};
use starknet_api::core::ascii_as_felt;
use starknet_types_core::felt::Felt;

use crate::hints::enum_definition::{AllHints, OsHint};
use crate::hints::error::OsHintResult;
use crate::hints::nondet_offsets::insert_nondet_hint_value;
use crate::hints::types::HintArgs;
use crate::hints::vars::{Const, Ids};

// Hint implementations.

pub(crate) fn block_number<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let block_number = hint_processor.execution_helper.os_input.block_info.block_number;
    Ok(insert_value_into_ap(vm, Felt::from(block_number.0))?)
}

pub(crate) fn block_timestamp<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let block_timestamp = hint_processor.execution_helper.os_input.block_info.block_timestamp;
    Ok(insert_value_into_ap(vm, Felt::from(block_timestamp.0))?)
}

pub(crate) fn chain_id<S: StateReader>(
    HintArgs { vm, hint_processor, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let chain_id = &hint_processor.execution_helper.os_input.chain_info.chain_id;
    let chain_id_as_felt = ascii_as_felt(&chain_id.to_string())?;
    Ok(insert_value_into_ap(vm, chain_id_as_felt)?)
}

pub(crate) fn fee_token_address<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let strk_fee_token_address = hint_processor
        .execution_helper
        .os_input
        .chain_info
        .fee_token_addresses
        .strk_fee_token_address;
    Ok(insert_value_into_ap(vm, strk_fee_token_address.0.key())?)
}

pub(crate) fn sequencer_address<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let address = hint_processor.execution_helper.os_input.block_info.sequencer_address;
    Ok(insert_value_into_ap(vm, address.0.key())?)
}

pub(crate) fn get_block_mapping<S: StateReader>(
    HintArgs { ids_data, constants, vm, ap_tracking, exec_scopes, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let block_hash_contract_address = Const::BlockHashContractAddress.fetch(constants)?;
    let contract_state_changes_ptr =
        get_ptr_from_var_name(Ids::ContractStateChanges.into(), vm, ids_data, ap_tracking)?;
    let dict_manager = exec_scopes.get_dict_manager()?;
    let mut dict_manager_borrowed = dict_manager.borrow_mut();
    let block_hash_state_entry = dict_manager_borrowed
        .get_tracker_mut(contract_state_changes_ptr)?
        .get_value(&block_hash_contract_address.into())?;

    Ok(insert_value_from_var_name(
        Ids::StateEntry.into(),
        block_hash_state_entry,
        vm,
        ids_data,
        ap_tracking,
    )?)
}

pub(crate) fn write_use_kzg_da_to_memory<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let use_kzg_da = hint_processor.execution_helper.os_input.block_info.use_kzg_da
        && !hint_processor.os_hints_config.full_output;

    insert_nondet_hint_value(
        vm,
        AllHints::OsHint(OsHint::WriteUseKzgDaToMemory),
        Felt::from(use_kzg_da),
    )
}
