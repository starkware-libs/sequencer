use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    insert_value_into_ap,
};
use starknet_types_core::felt::Felt;

use crate::hints::error::OsHintResult;
use crate::hints::types::HintArgs;
use crate::hints::vars::{Const, Ids};

#[allow(clippy::result_large_err)]
pub(crate) fn is_block_number_in_block_hash_buffer<S: StateReader>(
    HintArgs { ids_data, ap_tracking, vm, constants, .. }: HintArgs<'_, '_, S>,
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
