use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::insert_value_into_ap;
use starknet_api::core::ascii_as_felt;
use starknet_types_core::felt::Felt;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::enum_definition::{AllHints, OsHint};
use crate::hints::error::OsHintResult;
use crate::hints::hint_implementation::execution::utils::set_state_entry;
use crate::hints::nondet_offsets::insert_nondet_hint_value;
use crate::hints::types::HintArgs;
use crate::hints::vars::Const;

// Hint implementations.

pub(crate) fn block_number<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, .. }: HintArgs<'_>,
) -> OsHintResult {
    let block_number =
        hint_processor.get_current_execution_helper()?.os_block_input.block_info.block_number;
    Ok(insert_value_into_ap(vm, Felt::from(block_number.0))?)
}

pub(crate) fn block_timestamp<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, .. }: HintArgs<'_>,
) -> OsHintResult {
    let block_timestamp =
        hint_processor.get_current_execution_helper()?.os_block_input.block_info.block_timestamp;
    Ok(insert_value_into_ap(vm, Felt::from(block_timestamp.0))?)
}

pub(crate) fn chain_id<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, .. }: HintArgs<'_>,
) -> OsHintResult {
    let chain_id = &hint_processor.os_hints_config.chain_info.chain_id;
    let chain_id_as_felt = ascii_as_felt(&chain_id.to_string())?;
    Ok(insert_value_into_ap(vm, chain_id_as_felt)?)
}

pub(crate) fn fee_token_address<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, .. }: HintArgs<'_>,
) -> OsHintResult {
    let strk_fee_token_address = hint_processor.os_hints_config.chain_info.strk_fee_token_address;
    Ok(insert_value_into_ap(vm, strk_fee_token_address.0.key())?)
}

pub(crate) fn sequencer_address<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, .. }: HintArgs<'_>,
) -> OsHintResult {
    let address =
        hint_processor.get_current_execution_helper()?.os_block_input.block_info.sequencer_address;
    Ok(insert_value_into_ap(vm, address.0.key())?)
}

pub(crate) fn get_block_mapping(
    HintArgs { ids_data, constants, vm, ap_tracking, exec_scopes, .. }: HintArgs<'_>,
) -> OsHintResult {
    let block_hash_contract_address = Const::BlockHashContractAddress.fetch(constants)?;
    set_state_entry(block_hash_contract_address, vm, exec_scopes, ids_data, ap_tracking)
}

pub(crate) fn write_use_kzg_da_to_memory<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, .. }: HintArgs<'_>,
) -> OsHintResult {
    let use_kzg_da =
        hint_processor.os_hints_config.use_kzg_da && !hint_processor.os_hints_config.full_output;

    insert_nondet_hint_value(
        vm,
        AllHints::OsHint(OsHint::WriteUseKzgDaToMemory),
        Felt::from(use_kzg_da),
    )
}
