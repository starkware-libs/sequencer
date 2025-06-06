use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    insert_value_into_ap,
};
use starknet_types_core::felt::Felt;

use crate::hints::error::OsHintResult;
use crate::hints::types::HintArgs;
use crate::hints::vars::{CairoStruct, Ids};
use crate::vm_utils::get_address_of_nested_fields;

#[allow(clippy::result_large_err)]
pub(crate) fn is_on_curve<S: StateReader>(HintArgs { .. }: HintArgs<'_, '_, S>) -> OsHintResult {
    todo!()
}

#[allow(clippy::result_large_err)]
pub(crate) fn read_ec_point_from_address<S: StateReader>(
    HintArgs { ids_data, ap_tracking, vm, hint_processor, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let not_on_curve =
        get_integer_from_var_name(Ids::NotOnCurve.into(), vm, ids_data, ap_tracking)?;
    let ec_point_address = get_address_of_nested_fields(
        ids_data,
        Ids::Response,
        CairoStruct::SecpNewResponse,
        vm,
        ap_tracking,
        &["ec_point"],
        hint_processor.os_program,
    )?;
    let result =
        if not_on_curve == Felt::ZERO { ec_point_address } else { vm.add_memory_segment() };
    insert_value_into_ap(vm, result)?;
    Ok(())
}
