use blockifier::state::state_api::StateReader;
use starknet_types_core::felt::Felt;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::error::OsHintResult;
use crate::hints::types::HintContext;
use crate::hints::vars::{CairoStruct, Const, Ids};
use crate::vm_utils::get_address_of_nested_fields;

pub(crate) fn is_block_number_in_block_hash_buffer(mut ctx: HintContext<'_>) -> OsHintResult {
    let request_block_number = ctx.get_integer(Ids::RequestBlockNumber)?;
    let current_block_number = ctx.get_integer(Ids::CurrentBlockNumber)?;
    let stored_block_hash_buffer = ctx.fetch_const(Const::StoredBlockHashBuffer)?;
    let is_block_number_in_block_hash_buffer =
        request_block_number > current_block_number - stored_block_hash_buffer;
    ctx.insert_value(
        Ids::IsBlockNumberInBlockHashBuffer,
        Felt::from(is_block_number_in_block_hash_buffer),
    )?;
    Ok(())
}

pub(crate) fn relocate_sha256_segment<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
) -> OsHintResult {
    let state_ptr = ctx.vm.get_relocatable(get_address_of_nested_fields(
        ctx.ids_data,
        Ids::Response,
        CairoStruct::Sha256ProcessBlockResponsePtr,
        ctx.vm,
        ctx.ap_tracking,
        &["state_ptr"],
        hint_processor.program,
    )?)?;
    let actual_out_state_ptr = ctx.get_ptr(Ids::ActualOutState)?;

    // TODO(Nimrod): Use SHA256_STATE_SIZE_FELTS constant.
    let sha_state_size = 8;

    // Copy [state_ptr] into [actual_out_state_ptr] because finalize_sha256 will read it from
    // [actual_out_state_ptr] and the allocation is in the opposite direction.
    let data = ctx.vm.get_continuous_range(state_ptr, sha_state_size)?;
    ctx.vm.load_data(actual_out_state_ptr, &data)?;

    // Relocate segment.
    ctx.vm.add_relocation_rule(state_ptr, actual_out_state_ptr.into())?;
    Ok(())
}
