use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    get_ptr_from_var_name,
    insert_value_from_var_name,
};
use cairo_vm::hint_processor::hint_processor_utils::felt_to_usize;
use starknet_types_core::felt::Felt;

use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::types::HintArgs;
use crate::hints::vars::Ids;

// TODO(Nimrod): Delete this hint (should be implemented in the VM).
#[allow(clippy::result_large_err)]
pub(crate) fn search_sorted_optimistic(
    HintArgs { ids_data, ap_tracking, vm, .. }: HintArgs<'_>,
) -> OsHintResult {
    let array_ptr = get_ptr_from_var_name(Ids::ArrayPtr.into(), vm, ids_data, ap_tracking)?;
    let elm_size =
        felt_to_usize(&get_integer_from_var_name(Ids::ElmSize.into(), vm, ids_data, ap_tracking)?)?;

    if elm_size == 0 {
        return Err(OsHintError::AssertionFailed {
            message: format!("Invalid value for elm_size. Got: {elm_size}."),
        });
    }

    let n_elms =
        felt_to_usize(&get_integer_from_var_name(Ids::NElms.into(), vm, ids_data, ap_tracking)?)?;

    let key = &get_integer_from_var_name(Ids::Key.into(), vm, ids_data, ap_tracking)?;

    let mut index = n_elms;
    let mut exists = false;

    // TODO(Nimrod): Verify that it's ok to ignore the `__find_element_max_size` variable.
    for i in 0..n_elms {
        let address = (array_ptr + (elm_size * i))?;
        let value = vm.get_integer(address)?;

        if value.as_ref() >= key {
            index = i;
            exists = value.as_ref() == key;

            break;
        }
    }

    let exists_felt = Felt::from(exists);

    insert_value_from_var_name(Ids::Index.into(), index, vm, ids_data, ap_tracking)?;
    insert_value_from_var_name(Ids::Exists.into(), exists_felt, vm, ids_data, ap_tracking)?;

    Ok(())
}
