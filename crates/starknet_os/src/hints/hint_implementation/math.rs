use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    insert_value_from_var_name,
};
use starknet_types_core::felt::Felt;

use crate::hints::error::OsHintResult;
use crate::hints::types::HintArgs;
use crate::hints::vars::Ids;

#[allow(clippy::result_large_err)]
pub(crate) fn log2_ceil<S: StateReader>(
    HintArgs { vm, ap_tracking, ids_data, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let value = get_integer_from_var_name(Ids::Value.into(), vm, ids_data, ap_tracking)?;
    assert!(value != Felt::ZERO, "log2_ceil is not defined for zero.");
    let bits = (value - Felt::ONE).bits();
    insert_value_from_var_name(Ids::Res.into(), bits, vm, ids_data, ap_tracking)?;
    Ok(())
}
