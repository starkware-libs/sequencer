use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    get_ptr_from_var_name,
    insert_value_into_ap,
};
use starknet_types_core::felt::Felt;

use crate::hints::error::OsHintResult;
use crate::hints::types::HintArgs;
use crate::hints::vars::{Const, Ids};

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

pub(crate) fn relocate_sha256_segment(
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let syscall_ptr = get_ptr_from_var_name(Ids::SyscallPtr.into(), vm, ids_data, ap_tracking)?;
    let sha_state_start = vm.get_relocatable(syscall_ptr)?;
    // TODO(Nimrod): Use SHA256_STATE_SIZE_FELTS constant.
    let sha_state_size = 8;
    let data = vm.get_continuous_range(sha_state_start, sha_state_size)?;
    let res_ptr = get_ptr_from_var_name(Ids::Res.into(), vm, ids_data, ap_tracking)?;
    vm.load_data(res_ptr, &data)?;

    // Relocate segment.
    vm.add_relocation_rule(sha_state_start, res_ptr.into())?;

    Ok(())
}
