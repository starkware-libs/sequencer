use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    get_ptr_from_var_name,
    insert_value_into_ap,
};
use starknet_types_core::felt::Felt;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::error::OsHintResult;
use crate::hints::types::HintArgs;
use crate::hints::vars::{CairoStruct, Const, Ids};
use crate::vm_utils::get_address_of_nested_fields;

pub(crate) fn is_block_number_in_block_hash_buffer(
    HintArgs { vm, ids_data, ap_tracking, constants, .. }: HintArgs<'_>,
) -> OsHintResult {
    let request_block_number =
        get_integer_from_var_name(Ids::RequestBlockNumber.into(), vm, ids_data, ap_tracking)?;
    let current_block_number =
        get_integer_from_var_name(Ids::CurrentBlockNumber.into(), vm, ids_data, ap_tracking)?;
    let stored_block_hash_buffer = Const::StoredBlockHashBuffer.fetch(constants)?;
    let is_block_number_in_block_hash_buffer =
        request_block_number > current_block_number - stored_block_hash_buffer;
    insert_value_into_ap(vm, Felt::from(is_block_number_in_block_hash_buffer))?;
    Ok(())
}

pub(crate) fn relocate_sha256_segment<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let state_ptr = vm.get_relocatable(get_address_of_nested_fields(
        ids_data,
        Ids::Response,
        CairoStruct::Sha256ProcessBlockResponse,
        vm,
        ap_tracking,
        &["state_ptr"],
        hint_processor.program,
    )?)?;
    let actual_state_ptr =
        get_ptr_from_var_name(Ids::ActualStatePtr.into(), vm, ids_data, ap_tracking)?;

    // TODO(Nimrod): Use SHA256_STATE_SIZE_FELTS constant.
    let sha_state_size = 8;

    let data = vm.get_continuous_range(state_ptr, sha_state_size)?;
    vm.load_data(actual_state_ptr, &data)?;
    // Relocate segment.
    vm.add_relocation_rule(state_ptr, actual_state_ptr.into())?;

    Ok(())
}
