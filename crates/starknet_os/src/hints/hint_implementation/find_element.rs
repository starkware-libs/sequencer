use cairo_vm::hint_processor::hint_processor_utils::felt_to_usize;
use starknet_types_core::felt::Felt;

use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::types::HintArgs;
use crate::hints::vars::Ids;

// TODO(Nimrod): Delete this hint (should be implemented in the VM).
pub(crate) fn search_sorted_optimistic(mut ctx: HintArgs<'_>) -> OsHintResult {
    let array_ptr = ctx.get_ptr(Ids::ArrayPtr)?;
    let elm_size = felt_to_usize(&ctx.get_integer(Ids::ElmSize)?)?;

    if elm_size == 0 {
        return Err(OsHintError::AssertionFailed {
            message: format!("Invalid value for elm_size. Got: {elm_size}."),
        });
    }

    let n_elms = felt_to_usize(&ctx.get_integer(Ids::NElms)?)?;

    let key = &ctx.get_integer(Ids::Key)?;

    let mut index = n_elms;
    let mut exists = false;

    // TODO(Nimrod): Verify that it's ok to ignore the `__find_element_max_size` variable.
    for i in 0..n_elms {
        let address = (array_ptr + (elm_size * i))?;
        let value = ctx.vm.get_integer(address)?;

        if value.as_ref() >= key {
            index = i;
            exists = value.as_ref() == key;

            break;
        }
    }

    let exists_felt = Felt::from(exists);

    ctx.insert_value(Ids::Index, index)?;
    ctx.insert_value(Ids::Exists, exists_felt)?;

    Ok(())
}
